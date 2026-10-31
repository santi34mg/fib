//! Transactional function construction. Dropping an unfinished builder changes no arenas.
use super::*;
use std::collections::HashSet;

#[derive(Debug)]
pub struct FunctionBuilder<'ir> {
    ir: &'ir mut Ir,
    working: Ir,
    function: FunctionId,
    blocks: Vec<BlockId>,
    slots: Vec<StackSlotId>,
    terminated: HashSet<BlockId>,
}

impl<'ir> FunctionBuilder<'ir> {
    /// Inspect the staged IR without publishing the transaction.
    pub fn ir(&self) -> &Ir {
        &self.working
    }
    /// Update staged arenas during lowering; `finish` verifies before publishing.
    pub fn ir_mut(&mut self) -> &mut Ir {
        &mut self.working
    }

    /// Intern a constant in this transaction's staged constant arena.
    pub fn constant(&mut self, ty: TypeId, kind: ConstantKind) -> Operand {
        let id = self
            .working
            .constants
            .iter()
            .position(|c| c.ty == ty && c.kind == kind)
            .unwrap_or_else(|| {
                let id = self.working.constants.len();
                self.working.constants.push(Constant { ty, kind });
                id
            });
        Operand::Constant(ConstantId(id))
    }

    pub fn new(ir: &'ir mut Ir, function: FunctionId) -> IrResult<Self> {
        if !matches!(
            ir.get_function(function).map(|f| &f.body),
            Some(FunctionBody::Declaration)
        ) {
            return Err(IrError::Invalid {
                message: "builder requires an existing declaration",
            });
        }
        Ok(Self {
            working: ir.clone(),
            ir,
            function,
            blocks: vec![],
            slots: vec![],
            terminated: HashSet::new(),
        })
    }

    pub fn create_block(&mut self, params: &[TypeId]) -> IrResult<BlockId> {
        self.check_types(params)?;
        let block = BlockId(self.working.blocks.len());
        let params = params
            .iter()
            .enumerate()
            .map(|(index, &ty)| {
                let id = ValueId(self.working.values.len());
                self.working.values.push(Value {
                    owner: self.function,
                    ty,
                    definition: ValueDefinition::BlockParameter { block, index },
                });
                id
            })
            .collect();
        self.working.blocks.push(BasicBlock {
            owner: self.function,
            params,
            instructions: vec![],
            terminator: Terminator::Unreachable,
        });
        self.blocks.push(block);
        Ok(block)
    }

    fn check_types(&self, types: &[TypeId]) -> IrResult<()> {
        if types.iter().any(|&ty| self.working.get_type(ty).is_none()) {
            return Err(IrError::Invalid {
                message: "unknown type",
            });
        }
        Ok(())
    }

    fn check_block(&self, block: BlockId) -> IrResult<()> {
        if !self.blocks.contains(&block) {
            return Err(IrError::Invalid {
                message: "block belongs to another function",
            });
        }
        Ok(())
    }

    pub fn block_params(&self, block: BlockId) -> IrResult<&[ValueId]> {
        self.check_block(block)?;
        Ok(&self.working.blocks[block.0].params)
    }

    pub fn create_stack_slot(
        &mut self,
        ty: TypeId,
        debug_name: Option<String>,
    ) -> IrResult<StackSlotId> {
        self.check_types(&[ty])?;
        let id = StackSlotId(self.working.stack_slots.len());
        self.working.stack_slots.push(StackSlot {
            owner: self.function,
            ty,
            debug_name,
        });
        self.slots.push(id);
        Ok(id)
    }

    pub fn append_instruction(
        &mut self,
        block: BlockId,
        kind: InstructionKind,
        result_types: &[TypeId],
    ) -> IrResult<Vec<ValueId>> {
        self.check_block(block)?;
        self.check_types(result_types)?;
        if self.terminated.contains(&block) {
            return Err(IrError::Invalid {
                message: "insertion after terminator",
            });
        }
        let instruction = self.working.blocks[block.0].instructions.len();
        let results: Vec<_> = result_types
            .iter()
            .enumerate()
            .map(|(result, &ty)| {
                let id = ValueId(self.working.values.len());
                self.working.values.push(Value {
                    owner: self.function,
                    ty,
                    definition: ValueDefinition::InstructionResult {
                        block,
                        instruction,
                        result,
                    },
                });
                id
            })
            .collect();
        self.working.blocks[block.0].instructions.push(Instruction {
            results: results.clone(),
            kind,
        });
        Ok(results)
    }

    pub fn set_terminator(&mut self, block: BlockId, terminator: Terminator) -> IrResult<()> {
        self.check_block(block)?;
        if !self.terminated.insert(block) {
            return Err(IrError::Invalid {
                message: "block already terminated",
            });
        }
        self.working.blocks[block.0].terminator = terminator;
        Ok(())
    }

    pub fn finish(mut self, entry: BlockId) -> IrResult<FunctionId> {
        self.check_block(entry)?;
        if self.blocks.len() != self.terminated.len() {
            return Err(IrError::Invalid {
                message: "unterminated block",
            });
        }
        self.working.functions[self.function.0].body = FunctionBody::Definition {
            entry,
            blocks: self.blocks,
            stack_slots: self.slots,
        };
        verify_ir(&self.working)?;
        *self.ir = self.working;
        Ok(self.function)
    }
}
