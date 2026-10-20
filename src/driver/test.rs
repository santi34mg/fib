use super::*;
use crate::frontend::{namespace::ProjectSymbols, semantic_type::SemanticTypes};

fn source(text: &str) -> (tempfile::TempDir, BuildPlan) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("program.fib");
    std::fs::write(&path, text).unwrap();
    let plan = BuildPlan::single_file(&path).unwrap();
    (directory, plan)
}

#[test]
fn frontend_preserves_file_identity() {
    let (_directory, plan) = source("fn main() int32 { return 42; }");
    let project = run_frontend(&plan).unwrap();
    assert_eq!(project.units[0].source_file_id, plan.entry);
    crate::frontend::analyzer::validate_analysis(&project).unwrap();
    assert!(!plan.output.exists());
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
fn lowering_sketch_does_not_return_an_empty_success() {
    let project = AnalyzedProject {
        units: Vec::new(),
        symbols: ProjectSymbols {
            files_by_name: Default::default(),
            namespaces: Default::default(),
        },
        types: SemanticTypes::default(),
        source_paths: Default::default(),
    };
    let result = lower_to_ir(&project);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not implemented"));
}
