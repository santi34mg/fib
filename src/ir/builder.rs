//! Function-scoped construction API sketch. No ID allocation, SSA algorithm,
//! mutation, block sealing, or validation is implemented here.

use super::{
    BlockId, FunctionId, InstructionKind, Ir, IrError, IrResult, StackSlotId, Terminator, TypeId,
    ValueId,
};

/// A future builder borrows one IR and owns construction of one definition.
/// Its private state is intentionally unavailable until `new` is implemented.
#[derive(Debug)]
pub struct FunctionBuilder<'ir> {
    _ir: &'ir mut Ir,
    _function: FunctionId,
}

impl<'ir> FunctionBuilder<'ir> {
    /// Start a body for an existing function declaration. Must eventually reject
    /// an already defined function and keep unfinished blocks out of final IR.
    pub fn new(_ir: &'ir mut Ir, _function: FunctionId) -> IrResult<Self> {
        Err(IrError::NotImplemented {
            operation: "FunctionBuilder::new",
        })
    }

    /// Reserve a block with typed parameter definitions. No entry block is
    /// chosen implicitly; `finish` receives the entry explicitly.
    pub fn create_block(&mut self, _params: &[TypeId]) -> IrResult<BlockId> {
        Err(IrError::NotImplemented {
            operation: "FunctionBuilder::create_block",
        })
    }

    pub fn block_params(&self, _block: BlockId) -> IrResult<&[ValueId]> {
        Err(IrError::NotImplemented {
            operation: "FunctionBuilder::block_params",
        })
    }

    pub fn create_stack_slot(
        &mut self,
        _ty: TypeId,
        _debug_name: Option<String>,
    ) -> IrResult<StackSlotId> {
        Err(IrError::NotImplemented {
            operation: "FunctionBuilder::create_stack_slot",
        })
    }

    /// Allocate fresh result IDs; never assign an existing value ID. Must
    /// eventually reject insertion after the block's terminator.
    pub fn append_instruction(
        &mut self,
        _block: BlockId,
        _kind: InstructionKind,
        _result_types: &[TypeId],
    ) -> IrResult<Vec<ValueId>> {
        Err(IrError::NotImplemented {
            operation: "FunctionBuilder::append_instruction",
        })
    }

    pub fn set_terminator(&mut self, _block: BlockId, _terminator: Terminator) -> IrResult<()> {
        Err(IrError::NotImplemented {
            operation: "FunctionBuilder::set_terminator",
        })
    }

    /// Publish the completed body, requiring every block to be terminated and
    /// entry parameters to match the signature. Does not stand in for verify_ir.
    pub fn finish(self, _entry: BlockId) -> IrResult<FunctionId> {
        Err(IrError::NotImplemented {
            operation: "FunctionBuilder::finish",
        })
    }
}
