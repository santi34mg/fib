//! Non-terminating SSA instruction emission, not AST expression evaluation.

use crate::ir::BlockId;

use super::{BackendError, context::FunctionPlan};

impl FunctionPlan<'_, '_> {
    /// Identifies the instruction in the verified IR rather than accepting a
    /// detached/unverified record. Will bind all result IDs exactly once.
    /// Calls use the same ABI plan as predeclarations and definition entries.
    pub fn emit_instruction(
        &mut self,
        _block: BlockId,
        _instruction_index: usize,
    ) -> Result<(), BackendError> {
        Err(BackendError::NotImplemented {
            operation: "emit_instruction",
        })
    }
}
