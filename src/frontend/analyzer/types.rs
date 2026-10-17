//! Type compatibility pass contracts; semantic data lives in `semantic_type`.

use crate::frontend::semantic_type::{SemanticTypes, TypeId};

use super::not_implemented;

/// Check assignment compatibility after alias normalization and inference.
/// This sketch does not claim that unequal or unresolved types are compatible.
pub fn check_assignable(
    _expected: TypeId,
    _found: TypeId,
    _types: &SemanticTypes,
) -> anyhow::Result<()> {
    not_implemented("type compatibility checking")
}
