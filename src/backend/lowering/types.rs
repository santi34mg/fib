//! Explicit target selection and target-dependent storage/value type planning.

use super::{BackendError, ir_lower::VerifiedIr};

/// Requested target, not validated target data. There is deliberately no host
/// default. LLVM's target machinery must derive and validate its data layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetSpec {
    pub triple: String,
    pub cpu: String,
    /// Target feature spellings must be checked, not silently ignored.
    pub features: Vec<String>,
}

/// Opaque validated target data, eventually including data layout, endianness,
/// address-space-specific pointer sizes/alignments and GEP index widths.
#[derive(Debug)]
pub struct TargetLayout {
    _private: (),
}

/// Storage/value representations and layouts indexed by this IR's TypeIds.
/// The borrow prevents mutation of the verified IR during planning/emission.
#[derive(Debug)]
pub struct TypePlan<'ir> {
    _input: &'ir VerifiedIr<'ir>,
    _target: &'ir TargetLayout,
}

pub fn resolve_target(_target: &TargetSpec) -> Result<TargetLayout, BackendError> {
    Err(BackendError::NotImplemented {
        operation: "resolve_target",
    })
}

/// Will reject unsupported/recursive-by-value types, not supply guessed sizes.
pub fn plan_types<'ir>(
    _input: &'ir VerifiedIr<'ir>,
    _target: &'ir TargetLayout,
) -> Result<TypePlan<'ir>, BackendError> {
    Err(BackendError::NotImplemented {
        operation: "plan_types",
    })
}
