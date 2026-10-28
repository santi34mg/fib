//! Checked array/slice projections. All guards precede offset truncation and GEPs.
use super::*;

impl Lowerer<'_, '_, '_> {
    fn integer_constant(&mut self, ty: TypeId, bits: u128) -> Operand {
        self.builder.constant(ty, ConstantKind::Integer { bits })
    }

    fn compare(
        &mut self,
        state: &State,
        op: BinaryOp,
        lhs: Operand,
        rhs: Operand,
    ) -> anyhow::Result<Operand> {
        self.instruction(
            state,
            InstructionKind::Binary { op, lhs, rhs },
            scalar_id(&BuiltinType::Boolean),
        )
    }

    fn guard(
        &mut self,
        state: &mut State,
        condition: Operand,
        reason: TrapReason,
    ) -> anyhow::Result<()> {
        let next = self.builder.create_block(&[])?;
        let fail = self.builder.create_block(&[])?;
        self.builder.set_terminator(
            state.block,
            Terminator::Branch {
                condition,
                then_edge: Edge {
                    target: next,
                    args: vec![],
                },
                else_edge: Edge {
                    target: fail,
                    args: vec![],
                },
            },
        )?;
        self.builder
            .set_terminator(fail, Terminator::Trap(reason))?;
        state.block = next;
        Ok(())
    }

    fn integer_cast(
        &mut self,
        state: &State,
        value: Operand,
        ty: TypeId,
    ) -> anyhow::Result<Operand> {
        if self.builder.ir().operand_type(value) == Some(ty) {
            return Ok(value);
        }
        self.instruction(
            state,
            InstructionKind::Cast {
                kind: CastKind::Numeric,
                operand: value,
            },
            ty,
        )
    }

    /// Normalize to uint128 only after rejecting negative signed operands.
    fn nonnegative(
        &mut self,
        state: &mut State,
        value: Operand,
        reason: TrapReason,
    ) -> anyhow::Result<Operand> {
        let ty = self.builder.ir().operand_type(value).expect("typed index");
        if matches!(
            self.builder.ir().get_type(ty),
            Some(IrType::Integer { signed: true, .. })
        ) {
            let zero = self.integer_constant(ty, 0);
            let valid = self.compare(state, BinaryOp::Ge, value, zero)?;
            self.guard(state, valid, reason)?;
        }
        self.integer_cast(state, value, scalar_id(&BuiltinType::UInt128))
    }

    fn array_storage(&mut self, state: &mut State, object: &Expression) -> anyhow::Result<Operand> {
        if self.is_place(object) {
            return self.place(state, object);
        }
        let value = self.value_expression(state, object)?;
        let ty = self.source.expression_type(object)?;
        let slot = self
            .builder
            .create_stack_slot(ty, Some("array temporary".into()))?;
        let address = self.instruction(
            state,
            InstructionKind::StackAddress(slot),
            self.source.pointer_type(ty)?,
        )?;
        self.builder.append_instruction(
            state.block,
            InstructionKind::Store {
                address: Address(address),
                value,
            },
            &[],
        )?;
        Ok(address)
    }

    fn indexed_data(
        &mut self,
        state: &mut State,
        object: &Expression,
    ) -> anyhow::Result<(Operand, Operand)> {
        let semantic = object.annotation.resolved.expect("typed receiver").ty;
        let (container, _) = self.source.types.container(semantic)?;
        match container {
            SemanticType::Array { element, length } => {
                let element = self.source.value_type(*element, object.span)?;
                let length = *length;
                let address = self.array_storage(state, object)?;
                let data = self.instruction(
                    state,
                    InstructionKind::ArrayData(Address(address)),
                    self.source.pointer_type(element)?,
                )?;
                let length =
                    self.integer_constant(scalar_id(&BuiltinType::Usize), u128::from(length));
                Ok((data, length))
            }
            SemanticType::Slice(element) => {
                let element = self.source.value_type(*element, object.span)?;
                let slice = self.value_expression(state, object)?;
                let data = self.instruction(
                    state,
                    InstructionKind::SliceData(slice),
                    self.source.pointer_type(element)?,
                )?;
                let length = self.instruction(
                    state,
                    InstructionKind::SliceLength(slice),
                    scalar_id(&BuiltinType::Usize),
                )?;
                Ok((data, length))
            }
            _ => Err(self.source.error(object.span, "receiver is not indexable")),
        }
    }

    pub(super) fn index_address(
        &mut self,
        state: &mut State,
        object: &Expression,
        index: &Expression,
    ) -> anyhow::Result<Operand> {
        let (data, length) = self.indexed_data(state, object)?;
        let value = self.value_expression(state, index)?;
        let value = self.nonnegative(state, value, TrapReason::IndexOutOfBounds)?;
        let length = self.integer_cast(state, length, scalar_id(&BuiltinType::UInt128))?;
        let valid = self.compare(state, BinaryOp::Lt, value, length)?;
        self.guard(state, valid, TrapReason::IndexOutOfBounds)?;
        let pointer_ty = self.builder.ir().operand_type(data).expect("typed data");
        let null = self.builder.constant(pointer_ty, ConstantKind::Null);
        let valid = self.compare(state, BinaryOp::Ne, data, null)?;
        self.guard(state, valid, TrapReason::NullPointerDereference)?;
        let index = self.integer_cast(state, value, scalar_id(&BuiltinType::Usize))?;
        self.instruction(
            state,
            InstructionKind::ElementAddress {
                base: Address(data),
                index,
            },
            pointer_ty,
        )
    }

    pub(super) fn slice(
        &mut self,
        state: &mut State,
        expr: &Expression,
        object: &Expression,
        start: Option<&Expression>,
        end: Option<&Expression>,
        inclusive: bool,
    ) -> anyhow::Result<Operand> {
        let (data, length) = self.indexed_data(state, object)?;
        // Evaluate both explicit bounds before checking them, preserving source order.
        let start = start.map(|e| self.value_expression(state, e)).transpose()?;
        let end = end.map(|e| self.value_expression(state, e)).transpose()?;
        let wide = scalar_id(&BuiltinType::UInt128);
        let length_wide = self.integer_cast(state, length, wide)?;
        let start = if let Some(value) = start {
            self.nonnegative(state, value, TrapReason::InvalidSlice)?
        } else {
            self.integer_constant(wide, 0)
        };
        let valid = self.compare(state, BinaryOp::Le, start, length_wide)?;
        self.guard(state, valid, TrapReason::InvalidSlice)?;
        let end = if let Some(value) = end {
            let value = self.nonnegative(state, value, TrapReason::InvalidSlice)?;
            let valid = self.compare(
                state,
                if inclusive {
                    BinaryOp::Lt
                } else {
                    BinaryOp::Le
                },
                value,
                length_wide,
            )?;
            self.guard(state, valid, TrapReason::InvalidSlice)?;
            if inclusive {
                let one = self.integer_constant(wide, 1);
                self.instruction(
                    state,
                    InstructionKind::Binary {
                        op: BinaryOp::Add,
                        lhs: value,
                        rhs: one,
                    },
                    wide,
                )?
            } else {
                value
            }
        } else {
            length_wide
        };
        let valid = self.compare(state, BinaryOp::Le, start, end)?;
        self.guard(state, valid, TrapReason::InvalidSlice)?;
        let start = self.integer_cast(state, start, scalar_id(&BuiltinType::Usize))?;
        let end = self.integer_cast(state, end, scalar_id(&BuiltinType::Usize))?;
        let length = self.instruction(
            state,
            InstructionKind::Binary {
                op: BinaryOp::Sub,
                lhs: end,
                rhs: start,
            },
            scalar_id(&BuiltinType::Usize),
        )?;
        let pointer_ty = self.builder.ir().operand_type(data).expect("typed data");
        let data = self.instruction(
            state,
            InstructionKind::ElementAddress {
                base: Address(data),
                index: start,
            },
            pointer_ty,
        )?;
        self.instruction(
            state,
            InstructionKind::Slice { data, length },
            self.source.expression_type(expr)?,
        )
    }
}
