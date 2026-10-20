//! Module predeclarations and per-function emission state, not source scopes.

use crate::ir::FunctionId;

use super::{BackendError, abi::AbiPlan, llvm_lower::LlvmArtifact};

/// Future function/global/constant maps keyed by IR IDs. All signatures come
/// from the shared ABI plan; missing identities cannot trigger declarations.
#[derive(Debug)]
pub struct ModulePlan<'plan> {
    _abi: &'plan AbiPlan,
}

/// Future value/stack-slot/block/PHI maps and pending edge fixups. A function
/// borrows the module exclusively; there are no mutable SSA variable slots.
#[derive(Debug)]
pub struct FunctionPlan<'module, 'plan> {
    _module: &'module mut ModulePlan<'plan>,
}

/// Will predeclare all functions/globals before resolving constants and bodies,
/// supporting recursion and forward global/function address constants.
pub fn predeclare<'plan>(_abi: &'plan AbiPlan) -> Result<ModulePlan<'plan>, BackendError> {
    Err(BackendError::NotImplemented {
        operation: "predeclare",
    })
}

impl<'plan> ModulePlan<'plan> {
    /// For a definition only: allocate all block entries and non-entry PHIs,
    /// unpack entry arguments using the ABI, and allocate explicit stack slots.
    pub fn setup_blocks_and_phis(
        &mut self,
        _function: FunctionId,
    ) -> Result<FunctionPlan<'_, 'plan>, BackendError> {
        Err(BackendError::NotImplemented {
            operation: "setup_blocks_and_phis",
        })
    }

    /// Will reject pending functions/fixups, run LLVM module verification, then
    /// return owned text. Never serialize a partial module as a valid artifact.
    pub fn finish(self) -> Result<LlvmArtifact, BackendError> {
        Err(BackendError::NotImplemented {
            operation: "finish_module",
        })
    }
}
