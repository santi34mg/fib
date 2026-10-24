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
        "fn main() int32 { x: int32; x = 42; return x; }",
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
        "fn main() int32 { x: int32; x = 42; return x; }",
        "fn main() int32 { if false { x := [1, 2]; }; return 42; }",
        "fn main() int32 { while (false) { }; return 42; }",
        "fn main() int32 { comp T: type = int32; return 42; }",
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
