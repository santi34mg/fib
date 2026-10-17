//! One ABI classifier shared by declarations, definitions, calls and returns.

use super::{BackendError, types::TypePlan};

/// Future per-signature plans, including function-pointer signatures, with
/// mappings from semantic parameters/results to physical arguments/results.
/// Owns no LLVM handles; representation/legalization decisions precede emission.
#[derive(Debug)]
pub struct AbiPlan<'plan> {
    _types: &'plan TypePlan<'plan>,
}

/// Will classify Fib/C conventions, aggregate passing, multiple results,
/// hidden result pointers, extension attributes and fixed variadic prefixes.
/// Call-site variadic tails also need target-specific classification/promotions.
pub fn classify_abis<'plan>(
    _types: &'plan TypePlan<'plan>,
) -> Result<AbiPlan<'plan>, BackendError> {
    Err(BackendError::NotImplemented {
        operation: "classify_abis",
    })
}
