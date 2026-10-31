//! Contracts that cross the frontend/SSA boundary.
pub use fibc::{builtins, driver, frontend, ir, plan, standard_library};
use frontend::ast::expression::ExpressionKind;
use frontend::{
    analysis_annotation::ValueCategory,
    analyzer::{AnalyzedProject, Analyzer, validate_analysis},
    namespace::ProjectSymbols,
};
use plan::FileId;

mod memory_intrinsics {
    use super::*;
    use crate::{
        driver::CompilationUnit,
        frontend::{ast::statement::StatementKind as S, parser::Parser, tokenizer::Tokenizer},
        plan::{BuildPlan, FileName, SourceFile},
    };
    use std::path::Path;

    fn analyze(text: &str) -> anyhow::Result<AnalyzedProject> {
        let ast = Parser::with_source(Tokenizer::new(text), Path::new("main.fib"), text).parse()?;
        let plan = BuildPlan {
            standard_library: crate::standard_library::StandardLibrary::disabled(),
            native: crate::plan::NativeInputs::default(),
            packages: Vec::new(),
            files: vec![SourceFile {
                id: FileId(0),
                name: FileName("main".into()),
                path: "main.fib".into(),
            }],
            entry: FileId(0),
            output: "unused".into(),
        };
        Analyzer::new(
            plan,
            vec![CompilationUnit {
                source_file_id: FileId(0),
                ast,
            }],
        )?
        .with_synthetic_module(crate::builtins::SyntheticModule::MEMORY)?
        .analyze()
    }

    #[test]
    fn sizeof_ir_rejects_invalid_type_and_result_annotations() {
        use crate::ir::{InstructionKind, IrType};
        let project = analyze("fn f() usize { return core::sizeof(int32); }").unwrap();
        let ir = crate::ir::lowering::lower(&project).unwrap();
        for invalid_type in [false, true] {
            let mut broken = ir.clone();
            let instruction = broken
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.instructions)
                .find(|i| matches!(i.kind, InstructionKind::Sizeof(_)))
                .unwrap();
            if invalid_type {
                instruction.kind = InstructionKind::Sizeof(crate::ir::TypeId(usize::MAX));
            } else {
                let bool_type = broken
                    .types
                    .iter()
                    .position(|t| matches!(t, IrType::Bool))
                    .unwrap();
                broken.values[instruction.results[0].0].ty = crate::ir::TypeId(bool_type);
            }
            assert!(format!("{:#}", crate::ir::verify_ir(&broken).unwrap_err()).contains("sizeof"));
        }
    }

    #[test]
    fn sizeof_accepts_types_and_rejects_values_and_unsized_types() {
        for text in [
            "fn f() usize { return core::sizeof(int32); }",
            "fn f(T: type) usize { return core::sizeof(T); } fn g() usize { return f(int64); }",
            "comp S := struct { a: uint8, b: uint64 }; fn f() usize { comp size := core::sizeof; return size(S); }",
            "fn f() usize { return core::sizeof(int32[3]); }",
        ] {
            let project = analyze(text).unwrap_or_else(|e| panic!("{text}: {e:#}"));
            validate_analysis(&project).unwrap();
            crate::ir::lowering::lower(&project).unwrap();
        }
        for (text, message) in [
            ("fn f() { x := core::sizeof(42); }", "type argument"),
            ("fn f() { x := core::sizeof(void); }", "sized runtime type"),
            ("fn f() { x := core::sizeof(type); }", "sized runtime type"),
            ("fn f() { x := core::sizeof(); }", "argument count"),
            (
                "fn f() { x := core::sizeof(int32, int64); }",
                "argument count",
            ),
            ("fn f() { x := sizeof(int32); }", "unresolved"),
            ("fn f() { x := std::memory::sizeof(int32); }", "unresolved"),
            (
                "fn f() { x: uint64 = core::sizeof(int32); }",
                "incompatible types",
            ),
        ] {
            let error = analyze(text).unwrap_err();
            assert!(format!("{error:#}").contains(message), "{text}: {error:#}");
        }
    }

    #[test]
    fn unchecked_places_preserve_pointer_qualifiers_and_initialization_checks() {
        for text in [
            "fn f(p: const *int32) int32 { return std::memory::unchecked_deref(p); }",
            "fn f(p: *int32) { std::memory::unchecked_deref(p) = 42; }",
            "comp S: type = struct { x: int32 }; fn f(p: *S) { std::memory::unchecked_deref(p).x = 42; }",
            "fn f(p: *int32) *int32 { return std::memory::unchecked_deref(p)&; }",
            "fn f(p: *int32) int32 { comp d := std::memory::unchecked_deref; return d(p); }",
        ] {
            analyze(text).unwrap_or_else(|e| panic!("{text}\n{e:#}"));
        }
        for (text, expected) in [
            (
                "fn f() { std::memory::unchecked_deref(); }",
                "argument count",
            ),
            (
                "fn f(p: *int32) { std::memory::unchecked_deref(p, p); }",
                "argument count",
            ),
            (
                "fn f() { x := std::memory::unchecked_deref(42); }",
                "pointer",
            ),
            (
                "fn f(p: *void) { x := std::memory::unchecked_deref(p); }",
                "opaque",
            ),
            (
                "fn f(p: const *int32) { std::memory::unchecked_deref(p) = 42; }",
                "mutable",
            ),
            (
                "fn f() { p: *int32; x := std::memory::unchecked_deref(p); }",
                "uninitialized",
            ),
            (
                "fn f() { d := std::memory::unchecked_deref; }",
                "runtime value",
            ),
            (
                "fn f() { d := [std::memory::unchecked_deref]; }",
                "runtime value",
            ),
            (
                "comp S: type = struct { x: int32, y: int32 }; fn f() S { s: S; s.x = 42; return s; }",
                "uninitialized",
            ),
        ] {
            let error = analyze(text).unwrap_err();
            assert!(format!("{error:#}").contains(expected), "{text}: {error:#}");
        }
    }

    #[test]
    fn intrinsic_place_annotations_are_independently_rechecked() {
        let mut project =
            analyze("fn f(p: *int32) { std::memory::unchecked_deref(p) = 42; }").unwrap();
        let S::FunctionDeclaration(f) = &mut project.units[0].ast.statements[0].kind else {
            panic!("function")
        };
        let S::Assignment { left, .. } = &mut f.body.as_mut().unwrap().statements[0].kind else {
            panic!("assignment")
        };
        left.annotation.resolved.as_mut().unwrap().category = ValueCategory::Value;
        let error = validate_analysis(&project).unwrap_err();
        assert!(
            format!("{error:#}").contains("intrinsic result annotation"),
            "{error:#}"
        );
    }

    #[test]
    fn constructor_field_cardinality_is_revalidated_before_lowering() {
        for duplicate in [false, true] {
            let mut project = analyze("comp S: type = struct { x: int32, y: int32 }; fn main() int32 { s := S { x: 1, y: 2 }; return s.x; }").unwrap();
            let S::FunctionDeclaration(f) = &mut project.units[0].ast.statements[1].kind else {
                panic!("function")
            };
            let S::VariableDeclaration(v) = &mut f.body.as_mut().unwrap().statements[0].kind else {
                panic!("binding")
            };
            let ExpressionKind::StructConstruct { fields, .. } =
                &mut v.expression.as_mut().unwrap().kind
            else {
                panic!("constructor")
            };
            if duplicate {
                fields[1] = fields[0].clone();
            } else {
                fields.pop();
            }
            assert!(
                format!("{:#}", validate_analysis(&project).unwrap_err()).contains("exactly once")
            );
            assert!(crate::ir::lowering::lower(&project).is_err());
        }
    }

    #[test]
    fn memory_namespace_uses_normal_module_registration() {
        let project = analyze("fn main() int32 { return 0; }").unwrap();
        let mut units = project.units;
        units[0].source_file_id = FileId(0);
        let plan = BuildPlan {
            standard_library: crate::standard_library::StandardLibrary::disabled(),
            native: crate::plan::NativeInputs::default(),
            packages: Vec::new(),
            files: vec![SourceFile {
                id: FileId(0),
                name: FileName("memory".into()),
                path: "memory.fib".into(),
            }],
            entry: FileId(0),
            output: "unused".into(),
        };
        let analyzer = Analyzer::new(plan, units).unwrap();
        analyzer
            .with_synthetic_module(crate::builtins::SyntheticModule::MEMORY)
            .unwrap()
            .analyze()
            .unwrap();
    }
}

mod package_lookup {
    use super::*;
    use crate::{
        driver::CompilationUnit,
        frontend::{parser::Parser, tokenizer::Tokenizer},
        plan::{BuildPlan, PackageId, PackageSpec, SourceFile},
    };
    use std::path::Path;

    fn fixture(main: &str) -> (BuildPlan, Vec<CompilationUnit>) {
        let sources = [
            ("/app/src/main.fib", main),
            (
                "/app/src/tools/Helper.fib",
                "pub(root) fn number() int32 { return 2; }",
            ),
            (
                "/dep/src/Math.fib",
                "pub fn add(value: int32) int32 { return Utils::values::number() + value; }",
            ),
            (
                "/util/src/values.fib",
                "pub fn number() int32 { return 40; }",
            ),
        ];
        let mut files = Vec::new();
        let mut units = Vec::new();
        for (index, (path, source)) in sources.iter().enumerate() {
            files.push(SourceFile {
                id: FileId(index as u32),
                name: "same_logical_name".to_string().into(),
                path: path.into(),
            });
            units.push(CompilationUnit {
                source_file_id: FileId(index as u32),
                ast: Parser::with_source(Tokenizer::new(source), Path::new(path), source)
                    .parse()
                    .unwrap(),
            });
        }
        (
            BuildPlan {
                standard_library: crate::standard_library::StandardLibrary::disabled(),
                native: Default::default(),
                files,
                entry: FileId(0),
                output: "unused".into(),
                packages: vec![
                    PackageSpec {
                        id: PackageId(1),
                        identity: "test:app@1".into(),
                        source_root: "/app/src".into(),
                        dependencies: [("Math-Dep".into(), PackageId(7))].into(),
                        files: vec![FileId(0), FileId(1)],
                    },
                    PackageSpec {
                        id: PackageId(7),
                        identity: "test:math@1".into(),
                        source_root: "/dep/src".into(),
                        dependencies: [("Utils".into(), PackageId(9))].into(),
                        files: vec![FileId(2)],
                    },
                    PackageSpec {
                        id: PackageId(9),
                        identity: "test:utils@1".into(),
                        source_root: "/util/src".into(),
                        dependencies: Default::default(),
                        files: vec![FileId(3)],
                    },
                ],
            },
            units,
        )
    }

    #[test]
    fn package_relative_dependencies_root_and_normalized_file_paths_share_resolution() {
        let (plan, units) = fixture(
            "comp math := MaTh_DeP::mATH; fn main() int32 { return math::add(root::TOOLS::helper::number()); }",
        );
        let project = Analyzer::new(plan, units).unwrap().analyze().unwrap();
        crate::ir::lowering::lower(&project).unwrap();
    }

    #[test]
    fn dependencies_cannot_see_each_others_private_environments_and_roots_are_protected() {
        for source in [
            "fn main() int32 { return Utils::values::number(); }",
            "fn main() int32 { return values::number(); }",
            "fn main() int32 { root := 1; return root; }",
            "fn main(math_dep: int32) int32 { return math_dep; }",
            "fn Math_Dep() int32 { return 1; }",
        ] {
            let (plan, units) = fixture(source);
            assert!(
                Analyzer::new(plan, units).unwrap().analyze().is_err(),
                "{source}"
            );
        }
    }

    #[test]
    fn normalized_namespace_collisions_and_invalid_segments_fail_before_analysis() {
        for path in [
            "/app/src/MAIN.fib",
            "/app/src/main/child.fib",
            "/app/src/bad name.fib",
            "/app/src/while.fib",
        ] {
            let (mut plan, _) = fixture("fn main() int32 { return 0; }");
            plan.files[1].path = path.into();
            assert!(ProjectSymbols::from_plan(&plan).is_err(), "{path}");
        }
        let (mut plan, _) = fixture("fn main() int32 { return 0; }");
        plan.packages[0]
            .dependencies
            .insert("math_dep".into(), PackageId(9));
        assert!(ProjectSymbols::from_plan(&plan).is_err());
        let (mut plan, _) = fixture("fn main() int32 { return 0; }");
        plan.files[0].path = "/app/src/Tools/main.fib".into();
        assert!(ProjectSymbols::from_plan(&plan).is_err());
    }

    #[test]
    fn source_and_intrinsic_declarations_share_one_module_and_reject_duplicates() {
        let (plan, units) = fixture(
            "pub fn number() int32 { return 42; } fn main() int32 { return root::main::syscall0(39) as int32; }",
        );
        let project = Analyzer::new(plan, units)
            .unwrap()
            .with_module_intrinsics(FileId(0), crate::builtins::SyntheticModule::OS)
            .unwrap()
            .analyze()
            .unwrap();
        crate::ir::lowering::lower(&project).unwrap();
        let (plan, units) = fixture("fn syscall0() int32 { return 0; }");
        let error = Analyzer::new(plan, units)
            .unwrap()
            .with_module_intrinsics(FileId(0), crate::builtins::SyntheticModule::OS)
            .unwrap()
            .analyze()
            .unwrap_err();
        assert!(format!("{error:#}").contains("duplicate identifier"));
        let (plan, units) =
            fixture("fn main() int32 { return KERNEL_CALLS::syscall0(39) as int32; }");
        let project = Analyzer::new(plan, units)
            .unwrap()
            .with_synthetic_module(crate::builtins::SyntheticModule {
                name: "Kernel-Calls",
                functions: crate::builtins::SyntheticModule::OS.functions,
            })
            .unwrap()
            .analyze()
            .unwrap();
        crate::ir::lowering::lower(&project).unwrap();
    }
}
