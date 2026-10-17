//! Explicit terminators, successor occurrences and deferred PHI edge patching.

use crate::ir::BlockId;

use super::{BackendError, context::FunctionPlan};

/// Identifies a successor occurrence, not just a (source, target) pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Successor {
    Jump,
    Then,
    Else,
    SwitchCase(usize),
    SwitchDefault,
}

/// Distinct arms may target the same block with different arguments. Fixups
/// retain this identity AND the actual emitted predecessor after splitting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EdgeSite {
    pub source: BlockId,
    pub successor: Successor,
}

impl FunctionPlan<'_, '_> {
    /// Will read the verified block terminator, emit its explicit control flow,
    /// and record each edge occurrence; returns use the shared ABI result plan.
    pub fn emit_terminator(&mut self, _block: BlockId) -> Result<(), BackendError> {
        Err(BackendError::NotImplemented {
            operation: "emit_terminator",
        })
    }

    /// Consumes a completed function: resolve pending edge arguments (including
    /// loop backedges), patch PHIs using final emitted predecessors, and check
    /// that every instruction/result/terminator and incoming edge was handled.
    pub fn patch_edges(self) -> Result<(), BackendError> {
        Err(BackendError::NotImplemented {
            operation: "patch_edges",
        })
    }
}
