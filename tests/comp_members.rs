use fibc::{
    driver::CompilationUnit,
    frontend::{
        analyzer::{AnalyzedProject, Analyzer},
        parser::Parser,
        tokenizer::Tokenizer,
    },
    plan::{BuildPlan, FileId, FileName, SourceFile},
};
use std::path::Path;

fn analyze(text: &str) -> anyhow::Result<AnalyzedProject> {
    analyze_files(&[("main", text)])
}

fn analyze_files(sources: &[(&str, &str)]) -> anyhow::Result<AnalyzedProject> {
    let mut files = Vec::new();
    let mut units = Vec::new();
    for (index, (name, text)) in sources.iter().enumerate() {
        let path = format!("{name}.fib");
        let id = FileId(index as u32);
        let ast = Parser::with_source(Tokenizer::new(text), Path::new(&path), text).parse()?;
        files.push(SourceFile {
            id,
            name: FileName((*name).into()),
            path: path.into(),
        });
        units.push(CompilationUnit {
            source_file_id: id,
            ast,
        });
    }
    let plan = BuildPlan {
        standard_library: fibc::standard_library::StandardLibrary::disabled(),
        native: Default::default(),
        packages: Vec::new(),
        files,
        entry: FileId(0),
        output: "unused".into(),
    };
    Analyzer::new(plan, units)?.analyze()
}

const DOG: &str = r#"
comp Dog := struct {
    volume: uint8,
    comp bark := fn(self: const *Dog) uint8 { return self.volume; };
    comp louder := fn(self: *Dog) void { self.volume = self.volume + 1; };
};
"#;

#[test]
fn comp_members_read_and_mutate_explicit_receivers() {
    let project = analyze(&format!("{DOG} fn main() int32 {{ dog := Dog {{ volume: 3 }}; Dog.louder(dog&); return Dog.bark(dog&) as int32; }}")).unwrap();
    fibc::ir::lowering::lower(&project).unwrap();
}

#[test]
fn comp_members_do_not_change_layout_and_are_function_values() {
    let project = analyze(&format!("{DOG} fn main() int32 {{ dog := Dog {{ volume: 3 }}; callback: fn(const *Dog) uint8 = Dog.bark; return callback(dog&) as int32; }}")).unwrap();
    for ty in &project.types.types {
        if let fibc::frontend::semantic_type::SemanticType::Struct(fields) = ty {
            assert_eq!(fields.len(), 1);
        }
    }
    fibc::ir::lowering::lower(&project).unwrap();
}

#[test]
fn comp_members_reject_overrides_and_instance_selection() {
    for (body, message) in [
        (
            "dog := Dog { volume: 1, bark: Dog.bark };",
            "cannot be initialized",
        ),
        (
            "dog := Dog { volume: 1 }; dog.bark(dog&);",
            "requires a type value",
        ),
        ("Dog.bark = Dog.bark;", "assignment"),
        (
            "const dog := Dog { volume: 1 }; Dog.louder(dog&);",
            "const *struct",
        ),
    ] {
        let error = analyze(&format!("{DOG} fn main() void {{ {body} }}")).unwrap_err();
        assert!(error.to_string().contains(message), "{body}: {error}");
    }
}

#[test]
fn comp_members_reject_invalid_bodies_and_collisions() {
    for (text, message) in [
        (
            "comp Dog := struct { volume: uint8, comp volume := fn() void {}; };",
            "duplicate struct member",
        ),
        (
            "comp Dog := struct { comp f := fn() void {}; comp f := fn() void {}; };",
            "duplicate struct member",
        ),
        (
            "comp Dog := struct { volume: uint8, comp f := fn(self: const *Dog) void { self.volume = 2; }; };",
            "readonly",
        ),
        (
            "comp Dog := struct { comp f := fn() int32 {}; };",
            "does not return",
        ),
        (
            "comp Dog := struct { comp f := fn() void { missing(); }; };",
            "missing",
        ),
    ] {
        let error = analyze(text).unwrap_err();
        assert!(error.to_string().contains(message), "{text}: {error}");
    }
}

#[test]
fn comp_members_support_fixed_helpers_and_explicit_adapters() {
    let project = analyze(
        r#"
comp Writer := struct {
    context: *void,
    write: fn(*void, uint8) int32,
    comp write_all := fn(self: Writer, value: uint8) int32 {
        return self.write(self.context, value) + self.write(self.context, value);
    };
};
comp PageAllocator := struct {
    comp write := fn(context: *void, value: uint8) int32 { return value as int32; };
    comp as_writer := fn() Writer { return Writer { context: null, write: PageAllocator.write }; };
};
fn main() int32 { return Writer.write_all(PageAllocator.as_writer(), 3); }
"#,
    )
    .unwrap();
    fibc::ir::lowering::lower(&project).unwrap();
}

#[test]
fn comp_members_support_recursion_siblings_and_generic_functions() {
    let project = analyze(
        r#"
comp Math := struct {
    comp identity := fn(T: type, value: T) T { return value; };
    comp down := fn(n: int32) int32 { if n == 0 { return Math.zero(); }; return Math.down(n - 1); };
    comp zero := fn() int32 { return Math.identity(int32, 0); };
};
fn main() int32 { return Math.down(4); }
"#,
    )
    .unwrap();
    fibc::ir::lowering::lower(&project).unwrap();
}

#[test]
fn comp_members_named_main_are_ordinary_internal_functions() {
    let project = analyze("comp App := struct { comp main := fn() int32 { return 3; }; }; fn main() int32 { return App.main(); }").unwrap();
    fibc::ir::lowering::lower(&project).unwrap();
}

#[test]
fn comp_members_support_type_aliases_and_global_callbacks() {
    let project = analyze(&format!(
        r#"{DOG}
comp Alias := Dog;
const callback: fn(const *Dog) uint8 = Alias.bark;
fn main() int32 {{ const dog := Alias {{ volume: 7 }}; return callback(dog&) as int32; }}
"#
    ))
    .unwrap();
    fibc::ir::lowering::lower(&project).unwrap();
}

#[test]
fn comp_members_have_no_lexical_names_or_runtime_captures() {
    for text in [
        "comp Dog := struct { comp bark := fn() void {}; }; fn f() void { bark(); }",
        "fn f() void { local := 2; comp Dog := struct { comp bark := fn() int32 { return local; }; }; }",
        "fn f() void { local := 2; comp Dog := struct { comp bark := fn() void { local = 3; }; }; }",
        "comp Dog := struct { comp bark := fn() void {}; }; fn f() void { x := Dog {}; y: int32 = x; }",
    ] {
        assert!(analyze(text).is_err(), "{text}");
    }
}

#[test]
fn comp_members_parser_rejects_malformed_declarations() {
    for text in [
        "comp Dog := struct { comp bark := fn(self) void {}; };",
        "comp Dog := struct { comp bark := fn() void {} };",
        "comp Dog := struct { comp bark := fn() void; };",
        "comp Dog := struct { comp bark fn() void {}; };",
        "comp Dog := struct { comp bark := fn() void {};",
    ] {
        assert!(
            Parser::with_source(Tokenizer::new(text), Path::new("main.fib"), text)
                .parse()
                .is_err(),
            "{text}"
        );
    }
}

#[test]
fn comp_members_work_on_local_types() {
    let project = analyze(r#"
fn main() int32 {
    comp Local := struct { value: int32, comp get := fn(self: const *Local) int32 { return self.value; }; };
    value := Local { value: 4 };
    return Local.get(value&);
}
"#).unwrap();
    fibc::ir::lowering::lower(&project).unwrap();
}

#[test]
fn comp_members_distinguish_binding_from_comp_function_execution() {
    let project = analyze(
        r#"
comp Factory := struct {
    comp identity := comp fn(T: type) type { return T; };
};
comp Number := Factory.identity(int32);
fn main() Number { return 0; }
"#,
    )
    .unwrap();
    fibc::ir::lowering::lower(&project).unwrap();
    let error = analyze("comp Factory := struct { comp identity := fn(T: type) type { return T; }; }; comp Number := Factory.identity(int32);").unwrap_err();
    assert!(error.to_string().contains("comp"), "{error}");
}
#[test]
fn comp_members_annotations_are_revalidated_before_lowering() {
    let project = analyze(&format!("{DOG} fn main() void {{}} ")).unwrap();
    let mut broken = project.clone();
    broken.types.comp_members.clear();
    assert!(fibc::ir::lowering::lower(&broken).is_err());
    let mut broken = project.clone();
    let member = fibc::frontend::ast::members::declarations(&project.units[0].ast.statements)[0];
    if let fibc::frontend::ast::statement::StatementKind::FunctionDeclaration(function) =
        &member.kind
    {
        broken
            .types
            .declarations
            .remove(&match function.signature.annotation.binding.unwrap() {
                fibc::frontend::analysis_annotation::BindingId::Declaration(id) => id,
                _ => unreachable!(),
            });
    }
    assert!(fibc::ir::lowering::lower(&broken).is_err());
}

#[test]
fn comp_members_resolve_across_files_without_structural_conformance() {
    let project = analyze_files(&[
        ("main", "fn main() int32 { dog := util::Dog { volume: 5 }; return util::Dog.bark(dog&) as int32; }"),
        ("util", &format!("pub {DOG}")),
    ]).unwrap();
    fibc::ir::lowering::lower(&project).unwrap();
    let error = analyze(&format!("{DOG} comp Cat := struct {{ volume: uint8 }}; fn f(dog: Dog) void {{}} fn main() void {{ f(Cat {{ volume: 1 }}); }}")).unwrap_err();
    assert!(error.to_string().contains("incompatible types"), "{error}");
    analyze("comp MyId := int; fn f(id: MyId) int { return id; } fn main() int { return f(0); }")
        .unwrap();
}

#[cfg(feature = "llvm")]
#[test]
fn comp_members_execute_receivers_callbacks_helpers_and_layout() {
    use fibc::driver::{PipelineGoal, run_pipeline};
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("main.fib");
    std::fs::write(&source, format!(r#"{DOG}
comp Writer := struct {{
    write: fn(int32) int32,
    comp write_all := fn(self: Writer, value: int32) int32 {{ return self.write(value) + self.write(value); }};
}};
comp Implementation := struct {{
    comp write := fn(value: int32) int32 {{ return value + 1; }};
}};
const callback: fn(const *Dog) uint8 = Dog.bark;
fn main() int32 {{
    if core::sizeof(Dog) != 1 {{ return 1; }};
    dog := Dog {{ volume: 3 }};
    Dog.louder(dog&);
    if callback(dog&) != 4 {{ return 2; }};
    if Dog.bark(dog&) != 4 {{ return 3; }};
    writer := Writer {{ write: Implementation.write }};
    if Writer.write_all(writer, 5) != 12 {{ return 4; }};
    return 0;
}}
"#)).unwrap();
    let mut plan = BuildPlan::single_file(&source).unwrap();
    plan.standard_library = fibc::standard_library::StandardLibrary::disabled();
    run_pipeline(&plan, PipelineGoal::Binary).unwrap();
    assert_eq!(
        std::process::Command::new(&plan.output)
            .status()
            .unwrap()
            .code(),
        Some(0)
    );
}
