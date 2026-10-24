//! Verification for scalar/struct SSA and typed stack memory. Unsupported representation/operation families fail closed.
use super::*;
use std::collections::HashSet;

fn require(ok: bool, message: &'static str) -> IrResult<()> {
    if ok {
        Ok(())
    } else {
        Err(IrError::Invalid { message })
    }
}
fn unsupported(operation: &'static str) -> IrError {
    IrError::NotImplemented { operation }
}

pub fn verify_ir(ir: &Ir) -> IrResult<()> {
    if !ir.globals.is_empty() {
        return Err(unsupported("verification of globals"));
    }
    let mut sized = HashSet::new();
    for (index, ty) in ir.types.iter().enumerate() {
        match ty {
            IrType::Bool | IrType::Bytes | IrType::Float(_) => (),
            IrType::Integer {
                width: IntegerWidth::Bits(bits),
                ..
            } => require((1..=128).contains(bits), "invalid integer width")?,
            IrType::Pointer { pointee } => {
                require(ir.get_type(*pointee).is_some(), "unknown pointee type")?;
                require(
                    !matches!(ir.get_type(*pointee), Some(IrType::Bytes)),
                    "Bytes has no memory representation",
                )?
            }
            IrType::Struct { fields, .. } => {
                for field in fields {
                    require(ir.get_type(*field).is_some(), "unknown field type")?;
                    require(
                        !matches!(ir.get_type(*field), Some(IrType::Bytes)),
                        "Bytes has no struct field representation",
                    )?;
                }
                check_sized(ir, TypeId(index), &mut sized)?;
            }
            _ => return Err(unsupported("verification of type family")),
        }
    }
    let valid_type = |ty: TypeId| require(ir.get_type(ty).is_some(), "unknown type");
    for c in &ir.constants {
        valid_type(c.ty)?;
        match (&c.kind, ir.get_type(c.ty)) {
            (ConstantKind::Bool(_), Some(IrType::Bool)) => (),
            (ConstantKind::Null, Some(IrType::Pointer { .. })) => (),
            (ConstantKind::Bytes(_), Some(IrType::Bytes)) => (),
            (
                ConstantKind::Integer { bits },
                Some(IrType::Integer {
                    width: IntegerWidth::Bits(width),
                    ..
                }),
            ) => require(
                *width == 128 || *bits < (1u128 << width),
                "integer constant exceeds width",
            )?,
            (ConstantKind::Float { bits }, Some(IrType::Float(width))) => {
                let width = match width {
                    FloatWidth::F16 => 16,
                    FloatWidth::F32 => 32,
                    FloatWidth::F64 => 64,
                    FloatWidth::F128 => 128,
                };
                require(
                    width == 128 || *bits < (1u128 << width),
                    "float constant exceeds width",
                )?;
            }
            _ => {
                return Err(IrError::Invalid {
                    message: "constant does not match type",
                });
            }
        }
    }
    for (function, builtin) in &ir.builtin_functions {
        require(
            !builtin.is_intrinsic(),
            "memory intrinsic must be lowered to IR operations",
        )?;
        let declaration = ir.get_function(*function).ok_or(IrError::Invalid {
            message: "unknown builtin function",
        })?;
        require(
            matches!(declaration.body, FunctionBody::Declaration),
            "builtin function must be a declaration",
        )?;
    }
    let mut names = HashSet::new();
    let mut owned_blocks = HashSet::new();
    let mut owned_slots = HashSet::new();
    let mut definitions = HashSet::new();
    for (fi, f) in ir.functions.iter().enumerate() {
        let owner = FunctionId(fi);
        require(
            !f.identifier.is_empty() && !f.identifier.contains('\0') && names.insert(&f.identifier),
            "invalid or duplicate symbol",
        )?;
        for &ty in f.signature.params.iter().chain(&f.signature.results) {
            valid_type(ty)?;
        }
        let FunctionBody::Definition {
            entry,
            blocks,
            stack_slots,
        } = &f.body
        else {
            continue;
        };
        for &slot in stack_slots {
            let record = ir.get_stack_slot(slot).ok_or(IrError::Invalid {
                message: "unknown stack slot",
            })?;
            require(
                owned_slots.insert(slot) && record.owner == owner,
                "stack slot ownership mismatch",
            )?;
            valid_type(record.ty)?;
            require(
                !matches!(ir.get_type(record.ty), Some(IrType::Bytes)),
                "unsupported stack slot type",
            )?;
        }
        let members: HashSet<_> = blocks.iter().copied().collect();
        require(
            members.len() == blocks.len() && members.contains(entry),
            "invalid body membership or entry",
        )?;
        let mut predecessors = vec![Vec::new(); ir.blocks.len()];
        for &bid in blocks {
            require(owned_blocks.insert(bid), "block has multiple owners")?;
            let b = ir.get_block(bid).ok_or(IrError::Invalid {
                message: "unknown block",
            })?;
            require(b.owner == owner, "block owner mismatch")?;
            let mut define = |id: ValueId, location: ValueDefinition| -> IrResult<()> {
                let v = ir.get_value(id).ok_or(IrError::Invalid {
                    message: "unknown value definition",
                })?;
                valid_type(v.ty)?;
                require(
                    definitions.insert(id) && v.owner == owner && v.definition == location,
                    "duplicate or inconsistent value definition",
                )
            };
            for (index, &id) in b.params.iter().enumerate() {
                define(id, ValueDefinition::BlockParameter { block: bid, index })?;
            }
            for (instruction, inst) in b.instructions.iter().enumerate() {
                for (result, &id) in inst.results.iter().enumerate() {
                    define(
                        id,
                        ValueDefinition::InstructionResult {
                            block: bid,
                            instruction,
                            result,
                        },
                    )?;
                }
            }
            for edge in edges(&b.terminator)? {
                require(
                    members.contains(&edge.target) && edge.target != *entry,
                    "invalid edge target or edge to entry",
                )?;
                predecessors[edge.target.0].push(bid);
            }
        }
        let entry_types: Vec<_> = ir.blocks[entry.0]
            .params
            .iter()
            .map(|id| ir.values[id.0].ty)
            .collect();
        require(
            entry_types == f.signature.params,
            "entry signature mismatch",
        )?;
        // Compute reachability first; disconnected code has no well-defined dominance here.
        let mut reachable = HashSet::from([*entry]);
        loop {
            let old = reachable.len();
            for &bid in blocks {
                if reachable.contains(&bid) {
                    for edge in edges(&ir.blocks[bid.0].terminator)? {
                        reachable.insert(edge.target);
                    }
                }
            }
            if old == reachable.len() {
                break;
            }
        }
        require(reachable == members, "unreachable block")?;
        let mut dom = vec![HashSet::new(); ir.blocks.len()];
        for &bid in blocks {
            dom[bid.0] = if bid == *entry {
                HashSet::from([bid])
            } else {
                members.clone()
            };
        }
        loop {
            let mut changed = false;
            for &bid in blocks {
                if bid == *entry {
                    continue;
                }
                let mut set = members.clone();
                for pred in &predecessors[bid.0] {
                    set.retain(|b| dom[pred.0].contains(b));
                }
                set.insert(bid);
                if set != dom[bid.0] {
                    dom[bid.0] = set;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        for &bid in blocks {
            let b = &ir.blocks[bid.0];
            let operand = |op: Operand, position: usize| -> IrResult<TypeId> {
                let ty = ir.operand_type(op).ok_or(IrError::Invalid {
                    message: "unknown operand",
                })?;
                if let Operand::Value(id) = op {
                    let v = &ir.values[id.0];
                    require(
                        v.owner == owner && definitions.contains(&id),
                        "foreign or undefined operand",
                    )?;
                    let (block, instruction) = match v.definition {
                        ValueDefinition::BlockParameter { block, .. } => (block, None),
                        ValueDefinition::InstructionResult {
                            block, instruction, ..
                        } => (block, Some(instruction)),
                    };
                    require(
                        dom[bid.0].contains(&block)
                            && (block != bid || instruction.is_none_or(|i| i < position)),
                        "operand does not dominate use",
                    )?;
                }
                Ok(ty)
            };
            for (position, inst) in b.instructions.iter().enumerate() {
                let results: Vec<_> = inst.results.iter().map(|v| ir.values[v.0].ty).collect();
                let expected = match &inst.kind {
                    InstructionKind::Constant(id) => {
                        vec![operand(Operand::Constant(*id), position)?]
                    }
                    InstructionKind::Binary { op, lhs, rhs }
                        if matches!(
                            op,
                            BinaryOp::Eq
                                | BinaryOp::Ne
                                | BinaryOp::Lt
                                | BinaryOp::Le
                                | BinaryOp::Gt
                                | BinaryOp::Ge
                        ) =>
                    {
                        let lhs = operand(*lhs, position)?;
                        let rhs = operand(*rhs, position)?;
                        require(lhs == rhs, "comparison operand type mismatch")?;
                        require(
                            matches!(
                                ir.get_type(lhs),
                                Some(IrType::Integer { .. } | IrType::Float(_))
                            ) || (matches!(
                                ir.get_type(lhs),
                                Some(IrType::Bool | IrType::Pointer { .. })
                            ) && matches!(op, BinaryOp::Eq | BinaryOp::Ne)),
                            "invalid comparison operand type",
                        )?;
                        require(
                            results.len() == 1
                                && matches!(ir.get_type(results[0]), Some(IrType::Bool)),
                            "comparison requires one Bool result",
                        )?;
                        results.clone()
                    }
                    InstructionKind::Binary { op, lhs, rhs } => {
                        let lhs = operand(*lhs, position)?;
                        let rhs = operand(*rhs, position)?;
                        let integer = matches!(ir.get_type(lhs), Some(IrType::Integer { .. }));
                        let shift = matches!(op, BinaryOp::Shl | BinaryOp::Shr);
                        require(
                            if shift {
                                integer && matches!(ir.get_type(rhs), Some(IrType::Integer { .. }))
                            } else {
                                lhs == rhs
                            },
                            "binary operand type mismatch",
                        )?;
                        require(
                            integer
                                || (matches!(ir.get_type(lhs), Some(IrType::Float(_)))
                                    && matches!(
                                        op,
                                        BinaryOp::Add
                                            | BinaryOp::Sub
                                            | BinaryOp::Mul
                                            | BinaryOp::Div
                                    )),
                            "invalid arithmetic operand type",
                        )?;
                        vec![lhs]
                    }
                    InstructionKind::Unary { op, operand: value } => {
                        let ty = operand(*value, position)?;
                        require(
                            match op {
                                UnaryOp::Neg => matches!(
                                    ir.get_type(ty),
                                    Some(IrType::Integer { .. } | IrType::Float(_))
                                ),
                                UnaryOp::Not => matches!(
                                    ir.get_type(ty),
                                    Some(IrType::Integer { .. } | IrType::Bool)
                                ),
                            },
                            "invalid unary operand type",
                        )?;
                        vec![ty]
                    }
                    InstructionKind::Cast {
                        kind: CastKind::Numeric,
                        operand: value,
                    } => {
                        let source = operand(*value, position)?;
                        require(
                            results.len() == 1
                                && [source, results[0]].iter().all(|ty| {
                                    matches!(
                                        ir.get_type(*ty),
                                        Some(IrType::Integer { .. } | IrType::Float(_))
                                    )
                                }),
                            "invalid numeric cast",
                        )?;
                        results.clone()
                    }
                    InstructionKind::StackAddress(slot) => {
                        require(stack_slots.contains(slot), "foreign stack slot")?;
                        let slot = ir.get_stack_slot(*slot).ok_or(IrError::Invalid {
                            message: "unknown stack slot",
                        })?;
                        require(slot.owner == owner, "foreign stack slot")?;
                        require(
                            results.len() == 1
                                && matches!(ir.get_type(results[0]), Some(IrType::Pointer { pointee }) if *pointee == slot.ty),
                            "stack address result type mismatch",
                        )?;
                        results.clone()
                    }
                    InstructionKind::Load { address } => {
                        vec![pointee(ir, operand(address.0, position)?)?]
                    }
                    InstructionKind::Store { address, value } => {
                        require(
                            pointee(ir, operand(address.0, position)?)?
                                == operand(*value, position)?,
                            "store pointee type mismatch",
                        )?;
                        vec![]
                    }
                    InstructionKind::Aggregate { elements } => {
                        require(results.len() == 1, "aggregate requires one result")?;
                        let fields = fields(ir, results[0])?;
                        let actual = elements
                            .iter()
                            .map(|e| operand(*e, position))
                            .collect::<IrResult<Vec<_>>>()?;
                        require(*fields == actual, "aggregate field signature mismatch")?;
                        results.clone()
                    }
                    InstructionKind::ExtractField { aggregate, field } => {
                        vec![field_type(ir, operand(*aggregate, position)?, *field)?]
                    }
                    InstructionKind::InsertField {
                        aggregate,
                        field,
                        value,
                    } => {
                        let ty = operand(*aggregate, position)?;
                        require(
                            field_type(ir, ty, *field)? == operand(*value, position)?,
                            "insert field type mismatch",
                        )?;
                        vec![ty]
                    }
                    InstructionKind::FieldAddress { base, field } => {
                        let field =
                            field_type(ir, pointee(ir, operand(base.0, position)?)?, *field)?;
                        require(
                            results.len() == 1
                                && matches!(ir.get_type(results[0]), Some(IrType::Pointer { pointee }) if *pointee == field),
                            "field address result type mismatch",
                        )?;
                        results.clone()
                    }
                    InstructionKind::Call { callee, args } => {
                        let callee = ir.get_function(*callee).ok_or(IrError::Invalid {
                            message: "unknown callee",
                        })?;
                        let args = args
                            .iter()
                            .map(|&a| operand(a, position))
                            .collect::<IrResult<Vec<_>>>()?;
                        require(
                            args == callee.signature.params,
                            "call argument signature mismatch",
                        )?;
                        callee.signature.results.clone()
                    }
                    _ => return Err(unsupported("verification of instruction")),
                };
                require(results == expected, "instruction result signature mismatch")?;
            }
            let position = b.instructions.len();
            for edge in edges(&b.terminator)? {
                let args = edge
                    .args
                    .iter()
                    .map(|&a| operand(a, position))
                    .collect::<IrResult<Vec<_>>>()?;
                let params: Vec<_> = ir.blocks[edge.target.0]
                    .params
                    .iter()
                    .map(|v| ir.values[v.0].ty)
                    .collect();
                require(args == params, "edge argument signature mismatch")?;
            }
            match &b.terminator {
                Terminator::Return { values } => {
                    let values = values
                        .iter()
                        .map(|&v| operand(v, position))
                        .collect::<IrResult<Vec<_>>>()?;
                    require(values == f.signature.results, "return signature mismatch")?;
                }
                Terminator::Branch { condition, .. } => require(
                    matches!(
                        ir.get_type(operand(*condition, position)?),
                        Some(IrType::Bool)
                    ),
                    "branch requires bool",
                )?,
                Terminator::Jump(_) | Terminator::Trap(_) | Terminator::Unreachable => (),
                Terminator::Switch { .. } => return Err(unsupported("verification of switch")),
            }
        }
    }
    require(
        owned_blocks.len() == ir.blocks.len()
            && definitions.len() == ir.values.len()
            && owned_slots.len() == ir.stack_slots.len(),
        "orphan blocks, values or stack slots",
    )
}

fn edges(terminator: &Terminator) -> IrResult<Vec<&Edge>> {
    Ok(match terminator {
        Terminator::Jump(edge) => vec![edge],
        Terminator::Branch {
            then_edge,
            else_edge,
            ..
        } => vec![then_edge, else_edge],
        Terminator::Return { .. } | Terminator::Trap(_) | Terminator::Unreachable => vec![],
        Terminator::Switch { .. } => return Err(unsupported("verification of switch")),
    })
}

fn check_sized(ir: &Ir, ty: TypeId, done: &mut HashSet<TypeId>) -> IrResult<()> {
    let mut active = HashSet::new();
    let mut pending = vec![(ty, false)];
    while let Some((ty, finished)) = pending.pop() {
        if done.contains(&ty) {
            continue;
        }
        if finished {
            active.remove(&ty);
            done.insert(ty);
            continue;
        }
        require(active.insert(ty), "recursive by-value struct")?;
        pending.push((ty, true));
        if let Some(IrType::Struct { fields, .. }) = ir.get_type(ty) {
            for &field in fields {
                pending.push((field, false));
            }
        }
    }
    Ok(())
}
fn pointee(ir: &Ir, ty: TypeId) -> IrResult<TypeId> {
    match ir.get_type(ty) {
        Some(IrType::Pointer { pointee }) => Ok(*pointee),
        _ => Err(IrError::Invalid {
            message: "memory address must be a pointer",
        }),
    }
}
fn fields(ir: &Ir, ty: TypeId) -> IrResult<&Vec<TypeId>> {
    match ir.get_type(ty) {
        Some(IrType::Struct { fields, .. }) => Ok(fields),
        _ => Err(IrError::Invalid {
            message: "field operation requires a struct",
        }),
    }
}
fn field_type(ir: &Ir, ty: TypeId, field: usize) -> IrResult<TypeId> {
    fields(ir, ty)?.get(field).copied().ok_or(IrError::Invalid {
        message: "field index out of bounds",
    })
}
