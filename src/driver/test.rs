use super::*;

fn source(text: &str) -> (tempfile::TempDir, BuildPlan) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("program.fib");
    std::fs::write(&path, text).unwrap();
    let plan = BuildPlan::single_file(&path).unwrap();
    (directory, plan)
}

#[test]
fn build_file_builtins_lower_to_verified_bytes_calls_and_interpret() {
    let (_directory, plan) = source(
        "fn build() int32 { build_api::add_source(\"src/main.fib\"); build_api::set_entry(\"main\"); build_api::set_output(\"build/app\"); return 0; }",
    );
    let ir = compile_build_file_to_ir(&plan).unwrap();
    crate::ir::verify_ir(&ir).unwrap();
    assert_eq!(ir.builtin_functions.len(), 3);
    assert!(
        ir.types
            .iter()
            .any(|ty| matches!(ty, crate::ir::IrType::Bytes))
    );
    assert_eq!(
        ir.constants
            .iter()
            .filter(|constant| matches!(constant.kind, crate::ir::ConstantKind::Bytes(_)))
            .count(),
        3
    );

    let build_file = plan.files[0].path.clone();
    let mut builder = crate::plan::PlanBuilder::from(build_file);
    crate::ir_interpreter::interpret_ir(ir, &mut builder).unwrap();
    assert_eq!(
        builder.modules,
        vec![std::path::PathBuf::from("src/main.fib")]
    );
    assert_eq!(builder.entry_name.as_deref(), Some("main"));
    assert_eq!(builder.output, Some(std::path::PathBuf::from("build/app")));
}

#[test]
fn frontend_and_ir_pipeline_preserve_file_identity() {
    let (_directory, plan) = source("fn main() int32 { return 42; }");
    let project = run_frontend(&plan).unwrap();
    assert_eq!(project.units[0].source_file_id, plan.entry);
    let ir = crate::ir::lowering::lower(&project).unwrap();
    crate::ir::verify_ir(&ir).unwrap();
    assert_eq!(ir.functions[0].identifier, "main");
    assert!(matches!(
        run_pipeline(&plan, PipelineGoal::Check).unwrap(),
        CompilationOutput::Checked
    ));
    assert!(!plan.output.exists());
}

#[test]
fn unsupported_programs_and_invalid_analysis_fail_before_artifacts() {
    for text in [
        "fn main() int64 { return 42; }",
        "fn other() int32 { return 42; }",
    ] {
        let (_directory, plan) = source(text);
        assert!(run_pipeline(&plan, PipelineGoal::Binary).is_err());
        assert!(!plan.output.exists());
    }
    let (_directory, plan) = source("fn main() int32 { return 42; }");
    let mut project = run_frontend(&plan).unwrap();
    project.types.complete = false;
    assert!(crate::ir::lowering::lower(&project).is_err());
}

#[test]
fn invalid_plans_missing_files_and_parse_errors_are_diagnostics() {
    let (_directory, mut plan) = source("fn main() int32 { return ;");
    assert!(parse_files(&plan).is_err());
    plan.entry = FileId(99);
    assert!(
        parse_files(&plan)
            .unwrap_err()
            .to_string()
            .contains("entry")
    );
    plan.entry = FileId(0);
    std::fs::remove_file(&plan.files[0].path).unwrap();
    assert!(
        parse_files(&plan)
            .unwrap_err()
            .to_string()
            .contains("reading")
    );
}

#[test]
fn helpers_conditionals_and_mutable_locals_lower_to_verified_ssa() {
    let (_directory, plan) = source(include_str!("../../tests/programs/conditionals.fib"));
    let project = run_frontend(&plan).unwrap();
    let ir = crate::ir::lowering::lower(&project).unwrap();
    crate::ir::verify_ir(&ir).unwrap();
    assert_eq!(ir.functions.len(), 2);
    assert!(ir.blocks.len() >= 5);
    assert!(ir.blocks.iter().any(|block| !block.params.is_empty()));
    assert!(
        ir.blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(
                i.kind,
                crate::ir::InstructionKind::Binary {
                    op: crate::ir::BinaryOp::Lt,
                    ..
                }
            ))
    );
    assert!(ir.stack_slots.is_empty());
}

#[test]
fn scalar_subset_rejections_are_located_backend_diagnostics_even_in_dead_code() {
    use crate::diagnostics::{CompilerError, ErrorKind};
    for text in [
        "fn main() int32 { if false { x := [1, 2]; }; return 42; }",
        "fn main() int32 { while (false) { x := [1, 2]; }; return 42; }",
        "fn main() int32 { while () { break; x := [1, 2]; }; return 42; }",
        "fn main() int32 { while () { continue; x := [1, 2]; }; }",
        "fn main() int32 { x := 0; defer x = 1; return 42; }",
        "fn main() int32 { x, y := 1, 2; return x; }",
    ] {
        let (_directory, plan) = source(text);
        let project = run_frontend(&plan).unwrap();
        let error = crate::ir::lowering::lower(&project)
            .unwrap_err()
            .downcast::<CompilerError>()
            .unwrap();
        assert_eq!(error.kind, ErrorKind::Backend, "{text}");
        assert!(error.span.is_some(), "{text}");
        assert_eq!(
            error.filename.as_deref(),
            Some(plan.files[0].path.as_path()),
            "{text}"
        );
    }
}

#[test]
fn source_scope_and_return_shapes_are_supported_without_unreachable_ir() {
    for text in [
        "fn main() int32 { if true { return 42; }; }",
        "fn main() int32 { if (true == false) { return 7; } else { return 42; }; }",
        "fn f(x: int32) int32 { answer := 7; if (x == 1) { return 42; } else { answer = x; }; return answer; } fn main() int32 { return f(2); }",
        "fn main() int32 { x := 42; if false { x = 7; }; return x; }",
        "fn main() int32 { const x := 42; return x; }",
        "fn main() int32 { x := -2147483648; if (x < 0) { return 42; }; return 7; }",
        "fn main() int32 { x := 7; if true { local := 42; x = local; }; return x; }",
    ] {
        let (_directory, plan) = source(text);
        let project = run_frontend(&plan).unwrap();
        let ir = crate::ir::lowering::lower(&project).unwrap();
        crate::ir::verify_ir(&ir).unwrap();
    }
}

#[test]
fn frontend_const_initialization_and_scope_errors_remain_frontend_errors() {
    use crate::diagnostics::{CompilerError, ErrorKind};
    for (text, kind) in [
        (
            "fn main() int32 { const x := 42; x = 7; return x; }",
            ErrorKind::Type,
        ),
        ("fn main() int32 { x: int32; return x; }", ErrorKind::Type),
        (
            "fn main() int32 { if true { x := 42; }; return x; }",
            ErrorKind::Name,
        ),
    ] {
        let (_directory, plan) = source(text);
        let error = run_pipeline(&plan, PipelineGoal::Ir)
            .unwrap_err()
            .downcast::<CompilerError>()
            .unwrap();
        assert_eq!(error.kind, kind);
        assert!(error.span.is_some());
        assert!(!plan.output.exists());
    }
}

#[test]
fn all_scalar_types_lower_to_verified_ir_without_requiring_llvm() {
    use crate::ir::{CallingConvention, ConstantKind, FunctionBody, IrType, Linkage};
    let (_directory, plan) = source(include_str!("../../samples/builtin_scalars.fib"));
    let project = run_frontend(&plan).unwrap();
    let ir = crate::ir::lowering::lower(&project).unwrap();
    crate::ir::verify_ir(&ir).unwrap();
    assert!(
        ir.constants
            .iter()
            .any(|c| matches!(c.kind, ConstantKind::Integer { bits: u128::MAX }))
    );
    assert!(
        ir.constants
            .iter()
            .any(|c| matches!(c.kind, ConstantKind::Float { .. }))
    );
    assert_eq!(
        ir.types
            .iter()
            .filter(|ty| matches!(ty, IrType::Integer { .. }))
            .count(),
        10
    );
    assert_eq!(
        ir.types
            .iter()
            .filter(|ty| matches!(ty, IrType::Float(_)))
            .count(),
        4
    );
    for function in ir.functions.iter().filter(|f| f.identifier != "main") {
        assert_eq!(
            function.signature.calling_convention,
            CallingConvention::Fib
        );
        assert_eq!(function.linkage, Linkage::Internal);
        assert!(matches!(function.body, FunctionBody::Definition { .. }));
    }
    // The build-script entry spelling is an ordinary helper name in a program.
    let (_directory, plan) = source(
        "fn build(x: float128) float128 { return x; } fn main() int32 { if (build(1.0) == 1.0) { return 42; }; return 7; }",
    );
    let project = run_frontend(&plan).unwrap();
    let ir = crate::ir::lowering::lower(&project).unwrap();
    assert_eq!(
        ir.functions[0].signature.calling_convention,
        CallingConvention::Fib
    );
}

#[test]
fn loops_lower_to_verified_ssa_without_storage_or_unreachable_blocks() {
    for text in [
        include_str!("../../samples/scalar_loops.fib"),
        "fn main() int32 { while (false) {}; return 42; }",
        "fn main() int32 { while () {}; }",
        "fn main() int32 { x := 0; while (true) { x = x + 1; continue; }; }",
        "fn main() int32 { while (true) { return 42; }; }",
        "fn f(flag: bool) int32 { while () { if (flag) { return 42; } else { continue; }; }; } fn main() int32 { return f(true); }",
        "fn f(flag: bool) int32 { while (flag) { return 42; }; return 7; } fn main() int32 { return f(false); }",
        "fn main() int32 { x := 0; while (x < 3) { y := x + 1; x = y; }; return x; }",
        "fn main() int32 { x := 0; while (x < 3 && x < 5) { x = x + 1; continue; }; return x; }",
    ] {
        let (_directory, plan) = source(text);
        let project = run_frontend(&plan).unwrap();
        let ir = crate::ir::lowering::lower(&project).unwrap_or_else(|e| panic!("{text}\n{e:#}"));
        crate::ir::verify_ir(&ir).unwrap();
        assert!(ir.stack_slots.is_empty());
        // Stable local ordering also keeps repeated lowering deterministic.
        assert_eq!(
            ir.to_string(),
            crate::ir::lowering::lower(&project).unwrap().to_string()
        );
    }
}

#[test]
fn build_file_interpreter_rejects_loop_cfg_before_applying_operations() {
    let (_directory, plan) = source(
        "fn build() int32 { build_api::set_output(\"before-loop\"); while () { break; }; return 0; }",
    );
    let ir = compile_build_file_to_ir(&plan).unwrap();
    let mut builder = crate::plan::PlanBuilder::from(plan.files[0].path.clone());
    let error = crate::ir_interpreter::interpret_ir(ir, &mut builder).unwrap_err();
    assert!(error.to_string().contains("unsupported control flow"));
    assert!(builder.output.is_none());
}

#[test]
fn non_exiting_loops_have_no_synthetic_return_or_unreachable_terminator() {
    for text in [
        "fn main() int32 { while () {}; }",
        "fn main() int32 { x := 0; while (true) { x = x + 1; continue; }; }",
        "fn main() int32 { while (true) { if false { break; }; }; }",
    ] {
        let (_directory, plan) = source(text);
        let project = run_frontend(&plan).unwrap();
        let ir = crate::ir::lowering::lower(&project).unwrap();
        assert!(
            ir.blocks
                .iter()
                .all(|block| matches!(block.terminator, crate::ir::Terminator::Jump(_)))
        );
    }
}

#[test]
fn current_capabilities_project_lowers_without_llvm() {
    let directory = tempfile::tempdir().unwrap();
    let build_file = directory.path().join("build.fib");
    std::fs::write(&build_file, include_str!("../../example-project/build.fib")).unwrap();
    std::fs::write(
        directory.path().join("main.fib"),
        include_str!("../../example-project/main.fib"),
    )
    .unwrap();
    let plan = crate::plan::PlanBuilder::from(build_file)
        .compile_and_interpret_build_file()
        .unwrap();
    assert_eq!(plan.files.len(), 1);
    assert_eq!(
        plan.output,
        directory.path().canonicalize().unwrap().join("demo")
    );
    let CompilationOutput::Ir(ir) = run_pipeline(&plan, PipelineGoal::Ir).unwrap() else {
        panic!("expected verified IR")
    };
    crate::ir::verify_ir(&ir).unwrap();
    assert!(!plan.output.exists());
}

#[test]
fn struct_storage_sample_lowers_to_verified_memory_ir() {
    let (_directory, plan) = source(include_str!("../../samples/struct_storage.fib"));
    let project = run_frontend(&plan).unwrap();
    let ir = crate::ir::lowering::lower(&project).unwrap();
    crate::ir::verify_ir(&ir).unwrap();
    assert!(!ir.stack_slots.is_empty());
    assert!(
        ir.types
            .iter()
            .any(|t| matches!(t, crate::ir::IrType::Struct { .. }))
    );
    assert!(ir.builtin_functions.is_empty());
    assert_eq!(
        ir.to_string(),
        crate::ir::lowering::lower(&project).unwrap().to_string()
    );
}

#[test]
fn checked_and_unchecked_places_have_distinct_null_guard_ir() {
    use crate::ir::{Terminator, TrapReason};
    for (read, guards) in [("p*", 1), ("memory::unchecked_deref(p)", 0)] {
        let text = format!(
            "fn read(p: *int32) int32 {{ return {read}; }} fn main() int32 {{ x := 42; return read(x&); }}"
        );
        let (_directory, plan) = source(&text);
        let project = run_frontend(&plan).unwrap();
        let ir = crate::ir::lowering::lower(&project).unwrap();
        assert_eq!(
            ir.blocks
                .iter()
                .filter(|b| matches!(
                    b.terminator,
                    Terminator::Trap(TrapReason::NullPointerDereference)
                ))
                .count(),
            guards
        );
    }
    let text = "comp S: type = struct { next: *int32 }; fn read(p: *S) int32 { return memory::unchecked_deref(p).next*; } fn main() int32 { return 0; }";
    let (_directory, plan) = source(text);
    let ir = crate::ir::lowering::lower(&run_frontend(&plan).unwrap()).unwrap();
    assert_eq!(
        ir.blocks
            .iter()
            .filter(|b| matches!(
                b.terminator,
                Terminator::Trap(TrapReason::NullPointerDereference)
            ))
            .count(),
        1
    );
}

#[test]
fn unsupported_struct_fields_and_dead_memory_syntax_fail_before_artifacts() {
    for text in [
        "fn main() int32 { value: string; value = \"ready\"; return 0; }",
        "comp S: type = struct { a: int32[2] }; fn main() int32 { s := S { a: [1, 2] }; return 0; }",
        "comp S: type = struct { text: string }; fn main() int32 { s := S { text: \"a\" }; return 0; }",
        "fn main() int32 { while (false) { data := [1, 2]; }; return 0; }",
    ] {
        let (_directory, plan) = source(text);
        let project = run_frontend(&plan).unwrap();
        let error = crate::ir::lowering::lower(&project).unwrap_err();
        assert!(
            error
                .downcast_ref::<crate::diagnostics::CompilerError>()
                .is_some(),
            "{error:#}"
        );
        assert!(!plan.output.exists());
    }
}
