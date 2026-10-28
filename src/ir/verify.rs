//! Verification for scalar/struct/enum SSA and typed stack memory. Unsupported representation/operation families fail closed.
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
    let mut names = HashSet::new();
    for global in &ir.globals {
        require(
            !global.name.is_empty() && !global.name.contains('\0') && names.insert(&global.name),
            "invalid or duplicate global symbol",
        )?;
        valid_memory_type(ir, global.ty)?;
        match global.initializer {
            GlobalInitializer::Declaration => require(
                global.linkage == Linkage::External,
                "global declaration requires external linkage",
            )?,
            GlobalInitializer::Constant(id) => require(
                ir.get_constant(id).is_some_and(|c| c.ty == global.ty),
                "global initializer type mismatch",
            )?,
        }
    }
    let mut sized = HashSet::new();
    for (index, ty) in ir.types.iter().enumerate() {
        match ty {
            IrType::Opaque | IrType::Bool | IrType::Bytes | IrType::Float(_) => (),
            IrType::Slice { element } => {
                valid_memory_type(ir, *element)?;
            }
            IrType::Tuple(fields) => {
                for field in fields {
                    valid_memory_type(ir, *field)?;
                }
                check_sized(ir, TypeId(index), &mut sized)?;
            }
            IrType::Integer {
                width: IntegerWidth::Bits(bits),
                ..
            } => require((1..=128).contains(bits), "invalid integer width")?,
            IrType::Integer {
                width: IntegerWidth::Pointer,
                ..
            } => (),
            IrType::Array { element, .. } => {
                require(
                    ir.get_type(*element).is_some()
                        && !matches!(ir.get_type(*element), Some(IrType::Bytes | IrType::Opaque)),
                    "invalid array element type",
                )?;
                check_sized(ir, TypeId(index), &mut sized)?;
            }
            IrType::FunctionPointer(signature) => {
                for ty in signature.params.iter().chain(&signature.results) {
                    require(
                        ir.get_type(*ty).is_some(),
                        "unknown function pointer signature type",
                    )?;
                }
            }
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
                        !matches!(ir.get_type(*field), Some(IrType::Bytes | IrType::Opaque)),
                        "Bytes has no struct field representation",
                    )?;
                }
                check_sized(ir, TypeId(index), &mut sized)?;
            }
            IrType::Enum { variants, .. } => {
                let mut tags = HashSet::new();
                for variant in variants {
                    require(
                        tags.insert(variant.discriminant),
                        "duplicate enum discriminant",
                    )?;
                    for field in &variant.fields {
                        require(ir.get_type(*field).is_some(), "unknown enum field type")?;
                        require(
                            !matches!(ir.get_type(*field), Some(IrType::Bytes | IrType::Opaque)),
                            "Bytes has no enum field representation",
                        )?;
                    }
                }
                check_sized(ir, TypeId(index), &mut sized)?;
            }
        }
    }
    let valid_type = |ty: TypeId| require(ir.get_type(ty).is_some(), "unknown type");
    for c in &ir.constants {
        valid_type(c.ty)?;
        match (&c.kind, ir.get_type(c.ty)) {
            (ConstantKind::Bool(_), Some(IrType::Bool)) => (),
            (ConstantKind::Null, Some(IrType::Pointer { .. } | IrType::FunctionPointer(_))) => (),
            (ConstantKind::GlobalAddress(id), Some(IrType::Pointer { pointee })) => {
                require(
                    ir.get_global(*id).is_some_and(|g| g.ty == *pointee),
                    "global address type mismatch",
                )?;
            }
            (ConstantKind::PointerCast(base), Some(IrType::Pointer { .. })) => require(
                ir.get_constant(*base)
                    .is_some_and(|c| matches!(ir.get_type(c.ty), Some(IrType::Pointer { .. }))),
                "constant pointer cast requires pointer",
            )?,
            (ConstantKind::FunctionAddress(id), Some(IrType::FunctionPointer(signature))) => {
                require(
                    ir.get_function(*id)
                        .is_some_and(|f| &f.signature == signature),
                    "function address constant signature mismatch",
                )?
            }
            (
                ConstantKind::FieldAddress { base, field },
                Some(IrType::Pointer { pointee: result }),
            ) => {
                let base = ir.get_constant(*base).ok_or(IrError::Invalid {
                    message: "unknown constant address",
                })?;
                let base = pointee(ir, base.ty)?;
                let Some(IrType::Struct { fields, .. }) = ir.get_type(base) else {
                    return Err(IrError::Invalid {
                        message: "constant field address requires struct",
                    });
                };
                require(
                    fields.get(*field) == Some(result),
                    "constant field address type mismatch",
                )?;
            }
            (ConstantKind::ArrayData(base), Some(IrType::Pointer { pointee: result })) => {
                let base = ir.get_constant(*base).ok_or(IrError::Invalid {
                    message: "unknown constant address",
                })?;
                require(
                    matches!(ir.get_type(pointee(ir, base.ty)?), Some(IrType::Array { element, .. }) if element == result),
                    "constant array data type mismatch",
                )?;
            }
            (ConstantKind::ElementAddress { base, index }, Some(IrType::Pointer { pointee })) => {
                valid_memory_type(ir, *pointee)?;
                require(
                    ir.get_constant(*base).is_some_and(|base| base.ty == c.ty),
                    "constant element address type mismatch",
                )?;
                if let Some(remaining) = constant_array_remaining(ir, *base)? {
                    require(
                        *index <= remaining,
                        "constant element address out of bounds",
                    )?;
                }
            }
            (ConstantKind::Slice { data, length }, Some(IrType::Slice { element })) => {
                require(ir.get_constant(*data).is_some_and(|c| matches!(ir.get_type(c.ty), Some(IrType::Pointer { pointee }) if pointee == element)), "constant slice data type mismatch")?;
                if let Some(remaining) = constant_array_remaining(ir, *data)? {
                    require(*length <= remaining, "constant slice length out of bounds")?;
                }
            }
            (ConstantKind::Bytes(_), Some(IrType::Bytes)) => (),
            (ConstantKind::Bytes(bytes), Some(IrType::Array { element, len })) => require(
                bytes.len() as u64 == *len && byte_type(ir, *element),
                "byte array constant type mismatch",
            )?,
            (ConstantKind::Bytes(_), Some(IrType::Slice { element })) => {
                require(byte_type(ir, *element), "byte view constant type mismatch")?
            }
            (ConstantKind::CString(bytes), Some(IrType::Pointer { pointee })) => require(
                byte_type(ir, *pointee) && !bytes.contains(&0),
                "invalid C string constant",
            )?,
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
            (
                ConstantKind::Integer { .. },
                Some(IrType::Integer {
                    width: IntegerWidth::Pointer,
                    ..
                }),
            ) => (),
            (ConstantKind::Aggregate(elements), Some(IrType::Array { element, len })) => {
                require(
                    elements.len() as u64 == *len,
                    "array constant length mismatch",
                )?;
                for id in elements {
                    require(
                        ir.get_constant(*id).is_some_and(|c| c.ty == *element),
                        "array constant element type mismatch",
                    )?;
                }
            }
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
            (
                ConstantKind::Aggregate(elements),
                Some(IrType::Struct { fields, .. } | IrType::Tuple(fields)),
            ) => {
                constant_fields(ir, elements, fields)?;
            }
            (ConstantKind::Enum { variant, fields }, Some(IrType::Enum { variants, .. })) => {
                let variant = variants.get(*variant).ok_or(IrError::Invalid {
                    message: "invalid enum constant variant",
                })?;
                constant_fields(ir, fields, &variant.fields)?;
            }
            _ => {
                return Err(IrError::Invalid {
                    message: "constant does not match type",
                });
            }
        }
    }
    check_constant_cycles(ir)?;
    for (function, builtin) in &ir.builtin_functions {
        require(
            !builtin.is_intrinsic(),
            "compiler intrinsic must be lowered to IR operations",
        )?;
        let declaration = ir.get_function(*function).ok_or(IrError::Invalid {
            message: "unknown builtin function",
        })?;
        require(
            matches!(declaration.body, FunctionBody::Declaration),
            "builtin function must be a declaration",
        )?;
    }
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
            require(
                !matches!(ir.get_type(ty), Some(IrType::Opaque)),
                "opaque function value",
            )?;
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
                !matches!(ir.get_type(record.ty), Some(IrType::Bytes | IrType::Opaque)),
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
                    !matches!(ir.get_type(v.ty), Some(IrType::Opaque)),
                    "opaque SSA value",
                )?;
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
                                Some(
                                    IrType::Bool
                                        | IrType::Pointer { .. }
                                        | IrType::FunctionPointer(_)
                                )
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
                                        Some(
                                            IrType::Integer { .. }
                                                | IrType::Float(_)
                                                | IrType::Bool
                                        )
                                    )
                                }),
                            "invalid numeric cast",
                        )?;
                        results.clone()
                    }
                    InstructionKind::Cast {
                        kind: CastKind::Bitcast,
                        operand: value,
                    } => {
                        let from = operand(*value, position)?;
                        require(
                            results.len() == 1
                                && matches!(ir.get_type(from), Some(IrType::Pointer { .. }))
                                && matches!(ir.get_type(results[0]), Some(IrType::Pointer { .. })),
                            "bitcast requires pointer types",
                        )?;
                        results.clone()
                    }
                    InstructionKind::Cast {
                        kind: CastKind::IntegerToPointer,
                        operand: value,
                    } => {
                        let source = operand(*value, position)?;
                        require(
                            matches!(
                                ir.get_type(source),
                                Some(IrType::Integer {
                                    width: IntegerWidth::Bits(64),
                                    signed: false
                                })
                            ) && results.len() == 1
                                && matches!(ir.get_type(results[0]), Some(IrType::Pointer { .. })),
                            "integer-to-pointer cast requires uint64 and a pointer result",
                        )?;
                        results.clone()
                    }
                    InstructionKind::Syscall { number, args } => {
                        let number = operand(*number, position)?;
                        require(
                            matches!(
                                ir.get_type(number),
                                Some(IrType::Integer {
                                    width: IntegerWidth::Bits(64),
                                    signed: false
                                })
                            ),
                            "syscall number must be uint64",
                        )?;
                        require(args.len() <= 6, "syscall supports at most six arguments")?;
                        for arg in args {
                            let ty = operand(*arg, position)?;
                            require(
                                matches!(
                                    ir.get_type(ty),
                                    Some(
                                        IrType::Pointer { .. }
                                            | IrType::Integer {
                                                width: IntegerWidth::Bits(64),
                                                signed: false
                                            }
                                    )
                                ),
                                "syscall argument must be uint64 or a pointer",
                            )?;
                        }
                        require(
                            results.len() == 1
                                && matches!(
                                    ir.get_type(results[0]),
                                    Some(IrType::Integer {
                                        width: IntegerWidth::Bits(64),
                                        signed: true
                                    })
                                ),
                            "syscall result must be int64",
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
                    InstructionKind::GlobalAddress(id) => {
                        let global = ir.get_global(*id).ok_or(IrError::Invalid {
                            message: "unknown global",
                        })?;
                        require(
                            results.len() == 1
                                && matches!(ir.get_type(results[0]), Some(IrType::Pointer { pointee }) if *pointee == global.ty),
                            "global address result type mismatch",
                        )?;
                        results.clone()
                    }
                    InstructionKind::Load { address } => {
                        let ty = pointee(ir, operand(address.0, position)?)?;
                        valid_memory_type(ir, ty)?;
                        vec![ty]
                    }
                    InstructionKind::Store { address, value } => {
                        if let Some(global) = address_global(ir, address.0) {
                            require(
                                ir.get_global(global).is_some_and(|g| g.mutable),
                                "store to immutable or unknown global",
                            )?;
                        }
                        valid_memory_type(ir, pointee(ir, operand(address.0, position)?)?)?;
                        require(
                            pointee(ir, operand(address.0, position)?)?
                                == operand(*value, position)?,
                            "store pointee type mismatch",
                        )?;
                        vec![]
                    }
                    InstructionKind::Aggregate { elements } => {
                        require(results.len() == 1, "aggregate requires one result")?;
                        let actual = elements
                            .iter()
                            .map(|e| operand(*e, position))
                            .collect::<IrResult<Vec<_>>>()?;
                        match ir.get_type(results[0]) {
                            Some(IrType::Array { element, len }) => require(
                                actual.len() as u64 == *len
                                    && actual.iter().all(|ty| ty == element),
                                "array element signature mismatch",
                            )?,
                            _ => require(
                                *fields(ir, results[0])? == actual,
                                "aggregate field signature mismatch",
                            )?,
                        }
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
                    InstructionKind::ArrayData(base) => {
                        let array = pointee(ir, operand(base.0, position)?)?;
                        let Some(IrType::Array { element, .. }) = ir.get_type(array) else {
                            return Err(IrError::Invalid {
                                message: "array data requires pointer to array",
                            });
                        };
                        require(
                            results.len() == 1
                                && matches!(ir.get_type(results[0]), Some(IrType::Pointer { pointee }) if pointee == element),
                            "array data result mismatch",
                        )?;
                        results.clone()
                    }
                    InstructionKind::ElementAddress { base, index } => {
                        let ty = operand(base.0, position)?;
                        valid_memory_type(ir, pointee(ir, ty)?)?;
                        require(
                            matches!(
                                ir.get_type(operand(*index, position)?),
                                Some(IrType::Integer { .. })
                            ),
                            "element offset must be integer",
                        )?;
                        vec![ty]
                    }
                    InstructionKind::Slice { data, length } => {
                        let pointer = operand(*data, position)?;
                        let length = operand(*length, position)?;
                        require(
                            results.len() == 1
                                && matches!((ir.get_type(results[0]), ir.get_type(pointer)), (Some(IrType::Slice { element }), Some(IrType::Pointer { pointee })) if element == pointee),
                            "slice data signature mismatch",
                        )?;
                        require(
                            matches!(
                                ir.get_type(length),
                                Some(IrType::Integer {
                                    width: IntegerWidth::Pointer,
                                    signed: false
                                })
                            ),
                            "slice length requires usize",
                        )?;
                        results.clone()
                    }
                    InstructionKind::SliceData(value) => {
                        let Some(IrType::Slice { element }) =
                            ir.get_type(operand(*value, position)?)
                        else {
                            return Err(IrError::Invalid {
                                message: "slice data requires slice",
                            });
                        };
                        require(
                            results.len() == 1
                                && matches!(ir.get_type(results[0]),Some(IrType::Pointer {pointee}) if pointee == element),
                            "slice data result mismatch",
                        )?;
                        results.clone()
                    }
                    InstructionKind::SliceLength(value) => {
                        require(
                            matches!(
                                ir.get_type(operand(*value, position)?),
                                Some(IrType::Slice { .. })
                            ),
                            "slice length requires slice",
                        )?;
                        require(
                            results.len() == 1
                                && matches!(
                                    ir.get_type(results[0]),
                                    Some(IrType::Integer {
                                        width: IntegerWidth::Pointer,
                                        signed: false
                                    })
                                ),
                            "slice length result mismatch",
                        )?;
                        results.clone()
                    }
                    InstructionKind::Enum { variant, fields } => {
                        require(results.len() == 1, "enum requires one result")?;
                        let variant = enum_variant(ir, results[0], *variant)?;
                        let actual = fields
                            .iter()
                            .map(|f| operand(*f, position))
                            .collect::<IrResult<Vec<_>>>()?;
                        require(actual == variant.fields, "enum field signature mismatch")?;
                        results.clone()
                    }
                    InstructionKind::EnumTag { value } => {
                        let ty = operand(*value, position)?;
                        let Some(IrType::Enum { variants, .. }) = ir.get_type(ty) else {
                            return Err(IrError::Invalid {
                                message: "enum tag requires an enum",
                            });
                        };
                        require(results.len() == 1, "enum tag requires one result")?;
                        let Some(IrType::Integer {
                            width: IntegerWidth::Bits(width),
                            signed,
                        }) = ir.get_type(results[0])
                        else {
                            return Err(IrError::Invalid {
                                message: "enum tag requires an integer result",
                            });
                        };
                        let bits = width - u16::from(*signed);
                        require(
                            variants
                                .iter()
                                .all(|v| bits == 128 || v.discriminant < (1u128 << bits)),
                            "enum tag result cannot represent discriminants",
                        )?;
                        results.clone()
                    }
                    InstructionKind::EnumField {
                        value,
                        variant,
                        field,
                    } => {
                        let ty = operand(*value, position)?;
                        let selected = enum_variant(ir, ty, *variant)?;
                        let field_ty = *selected.fields.get(*field).ok_or(IrError::Invalid {
                            message: "enum field index out of bounds",
                        })?;
                        require(
                            active_variant(ir, *value, *variant, &dom[bid.0], &predecessors),
                            "enum extraction requires an established active variant",
                        )?;
                        vec![field_ty]
                    }
                    InstructionKind::FunctionAddress(function) => {
                        let function = ir.get_function(*function).ok_or(IrError::Invalid {
                            message: "unknown function address",
                        })?;
                        require(
                            results.len() == 1
                                && matches!(ir.get_type(results[0]), Some(IrType::FunctionPointer(signature)) if *signature == function.signature),
                            "function address signature mismatch",
                        )?;
                        results.clone()
                    }
                    InstructionKind::CallIndirect { callee, args } => {
                        let ty = operand(*callee, position)?;
                        let Some(IrType::FunctionPointer(signature)) = ir.get_type(ty) else {
                            return Err(IrError::Invalid {
                                message: "indirect call requires function pointer",
                            });
                        };
                        let arguments = args
                            .iter()
                            .map(|a| operand(*a, position))
                            .collect::<IrResult<Vec<_>>>()?;
                        require(
                            arguments == signature.params,
                            "indirect call argument signature mismatch",
                        )?;
                        signature.results.clone()
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
                Terminator::Switch { subject, cases, .. } => {
                    let ty = operand(*subject, position)?;
                    require(
                        matches!(ir.get_type(ty), Some(IrType::Integer { .. } | IrType::Bool)),
                        "switch requires integer or bool",
                    )?;
                    let mut seen = HashSet::new();
                    for case in cases {
                        let c = ir.get_constant(case.value).ok_or(IrError::Invalid {
                            message: "unknown switch constant",
                        })?;
                        require(c.ty == ty, "switch case type mismatch")?;
                        let bits = match c.kind {
                            ConstantKind::Integer { bits } => bits,
                            ConstantKind::Bool(b) => u128::from(b),
                            _ => {
                                return Err(IrError::Invalid {
                                    message: "switch case requires integer or bool constant",
                                });
                            }
                        };
                        require(seen.insert(bits), "duplicate switch case")?;
                    }
                }
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
        Terminator::Switch { cases, default, .. } => cases
            .iter()
            .map(|c| &c.edge)
            .chain(std::iter::once(default))
            .collect(),
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
        require(active.insert(ty), "recursive by-value struct or enum")?;
        pending.push((ty, true));
        match ir.get_type(ty) {
            Some(IrType::Struct { fields, .. } | IrType::Tuple(fields)) => {
                pending.extend(fields.iter().map(|f| (*f, false)))
            }
            Some(IrType::Array { element, .. }) => pending.push((*element, false)),
            Some(IrType::Enum { variants, .. }) => {
                pending.extend(variants.iter().flat_map(|v| &v.fields).map(|f| (*f, false)))
            }
            Some(IrType::Opaque | IrType::Bytes) => {
                return Err(IrError::Invalid {
                    message: "unsized memory type",
                });
            }
            _ => (),
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
        Some(IrType::Struct { fields, .. } | IrType::Tuple(fields)) => Ok(fields),
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

fn enum_variant(ir: &Ir, ty: TypeId, variant: usize) -> IrResult<&EnumVariant> {
    match ir.get_type(ty) {
        Some(IrType::Enum { variants, .. }) => variants.get(variant).ok_or(IrError::Invalid {
            message: "enum variant index out of bounds",
        }),
        _ => Err(IrError::Invalid {
            message: "enum operation requires an enum",
        }),
    }
}
fn constant_fields(ir: &Ir, values: &[ConstantId], fields: &[TypeId]) -> IrResult<()> {
    let actual = values
        .iter()
        .map(|id| {
            ir.get_constant(*id).map(|c| c.ty).ok_or(IrError::Invalid {
                message: "unknown aggregate constant field",
            })
        })
        .collect::<IrResult<Vec<_>>>()?;
    require(actual == fields, "constant field signature mismatch")
}
fn check_constant_cycles(ir: &Ir) -> IrResult<()> {
    let mut done = HashSet::new();
    let mut active = HashSet::new();
    for index in 0..ir.constants.len() {
        let mut pending = vec![(ConstantId(index), false)];
        while let Some((id, finish)) = pending.pop() {
            if done.contains(&id) {
                continue;
            }
            if finish {
                active.remove(&id);
                done.insert(id);
                continue;
            }
            require(active.insert(id), "cyclic constant graph")?;
            pending.push((id, true));
            match &ir.constants[id.0].kind {
                ConstantKind::Aggregate(fields) | ConstantKind::Enum { fields, .. } => {
                    pending.extend(fields.iter().map(|id| (*id, false)))
                }
                ConstantKind::FieldAddress { base, .. }
                | ConstantKind::ArrayData(base)
                | ConstantKind::PointerCast(base)
                | ConstantKind::ElementAddress { base, .. }
                | ConstantKind::Slice { data: base, .. } => pending.push((*base, false)),
                _ => (),
            }
        }
    }
    Ok(())
}
/// Recover a known inline-array extent without assuming allocation validity for
/// arbitrary raw pointers. The iteration limit also handles unverified cycles.
fn constant_array_remaining(ir: &Ir, mut id: ConstantId) -> IrResult<Option<u64>> {
    let mut offset = 0u128;
    for _ in 0..ir.constants.len() {
        match ir.get_constant(id).map(|c| &c.kind) {
            Some(ConstantKind::ElementAddress { base, index }) => {
                offset = offset
                    .checked_add(u128::from(*index))
                    .ok_or(IrError::Invalid {
                        message: "constant address offset overflows",
                    })?;
                id = *base;
            }
            Some(ConstantKind::ArrayData(base)) => {
                let Some(IrType::Array { len, .. }) = ir
                    .get_constant(*base)
                    .and_then(|c| pointee(ir, c.ty).ok())
                    .and_then(|ty| ir.get_type(ty))
                else {
                    return Ok(None);
                };
                require(
                    offset <= u128::from(*len),
                    "constant array address out of bounds",
                )?;
                return Ok(Some(*len - offset as u64));
            }
            _ => return Ok(None),
        }
    }
    Ok(None)
}
fn defining_instruction(ir: &Ir, value: Operand) -> Option<&InstructionKind> {
    let Operand::Value(id) = value else {
        return None;
    };
    let ValueDefinition::InstructionResult {
        block, instruction, ..
    } = ir.get_value(id)?.definition
    else {
        return None;
    };
    Some(&ir.get_block(block)?.instructions.get(instruction)?.kind)
}
fn address_global(ir: &Ir, mut value: Operand) -> Option<GlobalId> {
    for _ in 0..=ir.constants.len() + ir.values.len() {
        value = match value {
            Operand::Constant(id) => match &ir.get_constant(id)?.kind {
                ConstantKind::GlobalAddress(id) => return Some(*id),
                ConstantKind::FieldAddress { base, .. }
                | ConstantKind::ArrayData(base)
                | ConstantKind::ElementAddress { base, .. }
                | ConstantKind::PointerCast(base) => Operand::Constant(*base),
                _ => return None,
            },
            Operand::Value(_) => match defining_instruction(ir, value)? {
                InstructionKind::GlobalAddress(id) => return Some(*id),
                InstructionKind::FieldAddress { base, .. }
                | InstructionKind::ElementAddress { base, .. }
                | InstructionKind::ArrayData(base) => base.0,
                InstructionKind::Cast {
                    kind: CastKind::Bitcast,
                    operand,
                } => *operand,
                _ => return None,
            },
        };
    }
    None
}
fn active_variant(
    ir: &Ir,
    value: Operand,
    variant: usize,
    dominators: &HashSet<BlockId>,
    predecessors: &[Vec<BlockId>],
) -> bool {
    let mut definition = value;
    loop {
        match definition {
            Operand::Constant(id) => {
                if matches!(ir.get_constant(id).map(|c| &c.kind), Some(ConstantKind::Enum { variant: v, .. }) if *v == variant)
                {
                    return true;
                }
                break;
            }
            Operand::Value(_) => match defining_instruction(ir, definition) {
                Some(InstructionKind::Enum { variant: v, .. }) if *v == variant => return true,
                Some(InstructionKind::Constant(id)) => definition = Operand::Constant(*id),
                _ => break,
            },
        }
    }
    let Some(ty) = ir.operand_type(value) else {
        return false;
    };
    let Ok(selected) = enum_variant(ir, ty, variant) else {
        return false;
    };
    dominators.iter().any(|target| {
        let [source] = predecessors[target.0].as_slice() else { return false; };
        let Terminator::Switch { subject, cases, .. } = &ir.blocks[source.0].terminator else { return false; };
        if !matches!(defining_instruction(ir, *subject), Some(InstructionKind::EnumTag { value: v }) if *v == value) { return false; }
        cases.iter().any(|c| c.edge.target == *target && matches!(ir.get_constant(c.value).map(|c| &c.kind), Some(ConstantKind::Integer { bits }) if *bits == selected.discriminant))
    })
}

fn byte_type(ir: &Ir, ty: TypeId) -> bool {
    matches!(
        ir.get_type(ty),
        Some(IrType::Integer {
            width: IntegerWidth::Bits(8),
            signed: false
        })
    )
}
fn valid_memory_type(ir: &Ir, ty: TypeId) -> IrResult<()> {
    require(
        ir.get_type(ty).is_some()
            && !matches!(ir.get_type(ty), Some(IrType::Bytes | IrType::Opaque)),
        "unsized memory type",
    )
}
