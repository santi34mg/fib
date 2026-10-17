//! Verifier API only. No graph traversal, dominance or type checking yet.

use super::{Ir, IrError, IrResult};

/// Future checks include arena references/ownership, unique definitions and
/// dominance, edge arguments/block parameters, instruction types/result arity,
/// call/return signatures, declaration/body consistency and memory operands.
///
/// Always errors, even for an empty IR. Success must never mean "not checked".
pub fn verify_ir(_ir: &Ir) -> IrResult<()> {
    Err(IrError::NotImplemented {
        operation: "verify_ir",
    })
}
