use super::*;
use crate::frontend::{namespace::ProjectSymbols, semantic_type::SemanticTypes};

#[test]
fn pipeline_sketches_fail_explicitly_without_reading_files() {
    let plan = BuildPlan {
        files: Vec::new(),
        entry: FileId(0),
        output: "unused-output".into(),
    };
    for goal in [
        PipelineGoal::Check,
        PipelineGoal::Ir,
        PipelineGoal::Objects,
        PipelineGoal::Binary,
    ] {
        let result = run_pipeline(&plan, goal);
        assert!(result.is_err());
        assert!(
            result
                .err()
                .unwrap()
                .to_string()
                .contains("not implemented")
        );
    }
    for result in [
        run_frontend(&plan).map(|_| ()),
        parse_files(&plan).map(|_| ()),
    ] {
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not implemented"));
    }
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
    };
    let result = lower_to_ir(&project);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not implemented"));
}
