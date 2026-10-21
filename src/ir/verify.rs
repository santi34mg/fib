//! Verification for scalar SSA. Unsupported representation/operation families fail closed.
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
    if !ir.globals.is_empty() || !ir.stack_slots.is_empty() {
        return Err(unsupported("verification of storage"));
    }
    for ty in &ir.types {
        match ty {
            IrType::Bool | IrType::Bytes => (),
            IrType::Integer {
                width: IntegerWidth::Bits(bits),
                ..
            } => require((1..=128).contains(bits), "invalid integer width")?,
            _ => return Err(unsupported("verification of non-scalar types")),
        }
    }
    let valid_type = |ty: TypeId| require(ir.get_type(ty).is_some(), "unknown type");
    for c in &ir.constants {
        valid_type(c.ty)?;
        match (&c.kind, ir.get_type(c.ty)) {
            (ConstantKind::Bool(_), Some(IrType::Bool)) => (),
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
            _ => {
                return Err(IrError::Invalid {
                    message: "constant does not match type",
                });
            }
        }
    }
    for function in ir.builtin_functions.keys() {
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
        require(stack_slots.is_empty(), "unexpected stack slots")?;
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
                            matches!(ir.get_type(lhs), Some(IrType::Integer { .. }))
                                || (matches!(ir.get_type(lhs), Some(IrType::Bool))
                                    && matches!(op, BinaryOp::Eq | BinaryOp::Ne)),
                            "invalid comparison operand type",
                        )?;
                        require(
                            results.len() == 1
                                && matches!(ir.get_type(results[0]), Some(IrType::Bool)),
                            "comparison requires one Bool result",
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
                Terminator::Jump(_) | Terminator::Unreachable => (),
                Terminator::Switch { .. } => return Err(unsupported("verification of switch")),
            }
        }
    }
    require(
        owned_blocks.len() == ir.blocks.len() && definitions.len() == ir.values.len(),
        "orphan blocks or values",
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
        Terminator::Return { .. } | Terminator::Unreachable => vec![],
        Terminator::Switch { .. } => return Err(unsupported("verification of switch")),
    })
}
