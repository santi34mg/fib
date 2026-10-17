use crate::ir::{BlockId, Function, FunctionBody, Ir, Linkage, Signature};

use super::{
    BackendError, TargetSpec, emit_llvm,
    ir_lower::verify_input,
    statements::{EdgeSite, Successor},
    types::resolve_target,
};

fn target() -> TargetSpec {
    TargetSpec {
        triple: "x86_64-unknown-linux-gnu".into(),
        cpu: "generic".into(),
        features: Vec::new(),
    }
}

#[test]
fn emission_is_explicitly_unimplemented_even_for_empty_ir() {
    let ir = Ir::new();
    assert_eq!(
        emit_llvm(&ir, &target()),
        Err(BackendError::NotImplemented {
            operation: "emit_llvm",
        })
    );
    assert!(ir.is_empty());
}

#[test]
fn declaration_only_ir_does_not_produce_an_empty_artifact() {
    let mut ir = Ir::new();
    ir.functions.push(Function {
        name: "external_function".into(),
        signature: Signature {
            params: Vec::new(),
            results: Vec::new(),
            variadic: false,
            calling_convention: crate::ir::CallingConvention::C,
        },
        linkage: Linkage::External,
        body: FunctionBody::Declaration,
    });
    assert!(!ir.is_empty());
    assert!(matches!(
        emit_llvm(&ir, &target()),
        Err(BackendError::NotImplemented { .. })
    ));
    assert_eq!(ir.functions.len(), 1);
}

#[test]
fn verification_and_target_resolution_cannot_claim_success() {
    assert!(matches!(
        verify_input(&Ir::new()),
        Err(BackendError::NotImplemented {
            operation: "verify_input"
        })
    ));
    for triple in ["x86_64-unknown-linux-gnu", "i686-unknown-linux-gnu", ""] {
        let mut spec = target();
        spec.triple = triple.into();
        assert!(matches!(
            resolve_target(&spec),
            Err(BackendError::NotImplemented {
                operation: "resolve_target"
            })
        ));
    }
}

#[test]
fn edge_sites_distinguish_parallel_successors() {
    let sites = [
        Successor::Jump,
        Successor::Then,
        Successor::Else,
        Successor::SwitchCase(0),
        Successor::SwitchCase(1),
        Successor::SwitchDefault,
    ]
    .map(|successor| EdgeSite {
        source: BlockId(7),
        successor,
    });
    let unique: std::collections::HashSet<_> = sites.into_iter().collect();
    assert_eq!(unique.len(), sites.len());
}

#[test]
fn errors_identify_the_unfinished_operation() {
    let error = BackendError::NotImplemented {
        operation: "patch_edges",
    };
    let as_error: &dyn std::error::Error = &error;
    assert!(as_error.to_string().contains("patch_edges"));
    assert!(as_error.to_string().contains("not implemented"));
}
