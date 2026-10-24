//! Structural contract tests only: no frontend/lowering integration tests.

use super::*;

fn signature() -> Signature {
    Signature {
        params: vec![TypeId(0)],
        results: vec![TypeId(0)],
        calling_convention: CallingConvention::Fib,
    }
}

/// Hand-authored graph: one definition and one external declaration. This is
/// data for API tests, not an SSA construction algorithm or validity proof.
fn sample_ir() -> Ir {
    Ir {
        types: vec![
            IrType::Integer {
                width: IntegerWidth::Bits(32),
                signed: true,
            },
            IrType::Bool,
            IrType::Pointer { pointee: TypeId(0) },
        ],
        functions: vec![
            Function {
                identifier: "example".into(),
                signature: signature(),
                linkage: Linkage::Internal,
                body: FunctionBody::Definition {
                    entry: BlockId(0),
                    blocks: vec![BlockId(0), BlockId(1)],
                    stack_slots: vec![],
                },
            },
            Function {
                identifier: "external".into(),
                signature: Signature {
                    calling_convention: CallingConvention::C,
                    ..signature()
                },
                linkage: Linkage::External,
                body: FunctionBody::Declaration,
            },
        ],
        constants: vec![Constant {
            ty: TypeId(1),
            kind: ConstantKind::Bool(true),
        }],
        values: vec![
            Value {
                owner: FunctionId(0),
                ty: TypeId(0),
                definition: ValueDefinition::BlockParameter {
                    block: BlockId(0),
                    index: 0,
                },
            },
            Value {
                owner: FunctionId(0),
                ty: TypeId(0),
                definition: ValueDefinition::BlockParameter {
                    block: BlockId(1),
                    index: 0,
                },
            },
            Value {
                owner: FunctionId(0),
                ty: TypeId(0),
                definition: ValueDefinition::InstructionResult {
                    block: BlockId(1),
                    instruction: 0,
                    result: 0,
                },
            },
        ],
        blocks: vec![
            BasicBlock {
                owner: FunctionId(0),
                params: vec![ValueId(0)],
                instructions: vec![],
                terminator: Terminator::Branch {
                    condition: Operand::Constant(ConstantId(0)),
                    then_edge: Edge {
                        target: BlockId(1),
                        args: vec![Operand::Value(ValueId(0))],
                    },
                    else_edge: Edge {
                        target: BlockId(1),
                        args: vec![Operand::Value(ValueId(0))],
                    },
                },
            },
            BasicBlock {
                owner: FunctionId(0),
                params: vec![ValueId(1)],
                instructions: vec![Instruction {
                    results: vec![ValueId(2)],
                    kind: InstructionKind::Call {
                        callee: FunctionId(1),
                        args: vec![Operand::Value(ValueId(1))],
                    },
                }],
                terminator: Terminator::Return {
                    values: vec![Operand::Value(ValueId(2))],
                },
            },
        ],
        ..Ir::new()
    }
}

#[test]
fn arena_accessors_preserve_identity_ownership_and_types() {
    let ir = sample_ir();
    assert!(!ir.is_empty());
    assert_eq!(ir.get_block(BlockId(1)).unwrap().owner, FunctionId(0));
    assert_eq!(ir.get_value(ValueId(2)).unwrap().owner, FunctionId(0));
    assert_eq!(ir.operand_type(Operand::Value(ValueId(2))), Some(TypeId(0)));
    assert_eq!(
        ir.operand_type(Operand::Constant(ConstantId(0))),
        Some(TypeId(1))
    );
    assert!(matches!(
        ir.get_type(TypeId(0)),
        Some(IrType::Integer { signed: true, .. })
    ));
    assert!(ir.get_function(FunctionId(99)).is_none());
    assert!(ir.get_global(GlobalId(99)).is_none());
    assert!(ir.get_type(TypeId(99)).is_none());
    assert!(ir.get_constant(ConstantId(99)).is_none());
    assert!(ir.get_block(BlockId(99)).is_none());
    assert!(ir.get_value(ValueId(99)).is_none());
    assert!(ir.get_stack_slot(StackSlotId(99)).is_none());
    assert_eq!(ir.operand_type(Operand::Value(ValueId(99))), None);
}

#[test]
fn declarations_retain_signatures_without_blocks() {
    let mut ir = Ir::new();
    assert!(ir.is_empty());
    ir.functions.push(sample_ir().functions.remove(1));
    assert!(!ir.is_empty());
    assert!(ir.blocks.is_empty());
    let function = ir.get_function(FunctionId(0)).unwrap();
    assert!(matches!(function.body, FunctionBody::Declaration));
    assert_eq!(function.signature.params, vec![TypeId(0)]);
    assert_eq!(function.signature.results, vec![TypeId(0)]);
    assert_eq!(function.signature.calling_convention, CallingConvention::C);
}

#[test]
fn control_flow_carries_arguments_on_both_edges() {
    let ir = sample_ir();
    let Terminator::Branch {
        then_edge,
        else_edge,
        ..
    } = &ir.blocks[0].terminator
    else {
        panic!("expected explicit branch");
    };
    assert_eq!(then_edge.target, BlockId(1));
    assert_eq!(else_edge.target, BlockId(1));
    assert_eq!(then_edge.args, vec![Operand::Value(ValueId(0))]);
    assert_eq!(else_edge.args, then_edge.args);
    assert_eq!(ir.blocks[1].params, vec![ValueId(1)]);
    assert_eq!(ir.blocks[1].instructions[0].results, vec![ValueId(2)]);
}

#[test]
fn storage_and_ssa_values_are_distinct() {
    let slot = StackSlot {
        owner: FunctionId(0),
        ty: TypeId(0),
        debug_name: Some("local".into()),
    };
    let address = Instruction {
        results: vec![ValueId(0)],
        kind: InstructionKind::StackAddress(StackSlotId(0)),
    };
    let store = Instruction {
        results: vec![],
        kind: InstructionKind::Store {
            address: Address(Operand::Value(ValueId(0))),
            value: Operand::Constant(ConstantId(0)),
        },
    };
    let load = Instruction {
        results: vec![ValueId(1)],
        kind: InstructionKind::Load {
            address: Address(Operand::Value(ValueId(0))),
        },
    };
    assert_eq!(slot.owner, FunctionId(0));
    assert!(matches!(
        address.kind,
        InstructionKind::StackAddress(StackSlotId(0))
    ));
    assert!(store.results.is_empty());
    assert_ne!(address.results, load.results);
}

#[test]
fn verifier_accepts_scalar_graph_and_rejects_corruption() {
    let mut valid = sample_ir();
    valid.types.truncate(2);
    verify_ir(&valid).unwrap();
    let bad: Vec<fn(&mut Ir)> = vec![
        |ir| ir.blocks[1].owner = FunctionId(1),
        |ir| ir.values[2].owner = FunctionId(1),
        |ir| {
            ir.values[2].definition = ValueDefinition::BlockParameter {
                block: BlockId(1),
                index: 0,
            }
        },
        |ir| ir.blocks[1].instructions[0].results.push(ValueId(2)),
        |ir| ir.functions[0].signature.results.clear(),
        |ir| ir.functions[1].signature.params.clear(),
        |ir| ir.blocks[1].instructions[0].results.clear(),
        |ir| ir.blocks[1].instructions[0].kind = InstructionKind::Constant(ConstantId(99)),
        |ir| {
            ir.blocks[1].terminator = Terminator::Return {
                values: vec![Operand::Value(ValueId(99))],
            }
        },
        |ir| {
            ir.blocks[0].terminator = Terminator::Jump(Edge {
                target: BlockId(0),
                args: vec![],
            })
        },
        |ir| {
            ir.blocks[0].terminator = Terminator::Jump(Edge {
                target: BlockId(1),
                args: vec![],
            })
        },
        |ir| {
            ir.blocks[0].terminator = Terminator::Branch {
                condition: Operand::Value(ValueId(0)),
                then_edge: Edge {
                    target: BlockId(1),
                    args: vec![Operand::Value(ValueId(0))],
                },
                else_edge: Edge {
                    target: BlockId(1),
                    args: vec![Operand::Value(ValueId(0))],
                },
            }
        },
        |ir| ir.functions[1].identifier = "example".into(),
    ];
    for mutate in bad {
        let mut ir = valid.clone();
        mutate(&mut ir);
        assert!(verify_ir(&ir).is_err(), "accepted {ir:?}");
    }
    verify_ir(&Ir::new()).unwrap();
}

fn declaration_ir() -> Ir {
    let mut ir = Ir::new();
    ir.types.push(IrType::Integer {
        width: IntegerWidth::Bits(32),
        signed: true,
    });
    ir.functions.push(Function {
        identifier: "f".into(),
        signature: Signature {
            params: vec![],
            results: vec![TypeId(0)],
            calling_convention: CallingConvention::Fib,
        },
        linkage: Linkage::Internal,
        body: FunctionBody::Declaration,
    });
    ir.constants.push(Constant {
        ty: TypeId(0),
        kind: ConstantKind::Integer { bits: 42 },
    });
    ir
}

#[test]
fn builder_allocates_and_publishes_only_terminated_verified_bodies() {
    let mut ir = declaration_ir();
    {
        let mut builder = FunctionBuilder::new(&mut ir, FunctionId(0)).unwrap();
        let block = builder.create_block(&[]).unwrap();
        assert!(builder.finish(block).is_err());
    }
    assert!(ir.blocks.is_empty());
    assert!(ir.values.is_empty());
    let mut builder = FunctionBuilder::new(&mut ir, FunctionId(0)).unwrap();
    let block = builder.create_block(&[]).unwrap();
    let value = builder
        .append_instruction(
            block,
            InstructionKind::Constant(ConstantId(0)),
            &[TypeId(0)],
        )
        .unwrap()[0];
    builder
        .set_terminator(
            block,
            Terminator::Return {
                values: vec![Operand::Value(value)],
            },
        )
        .unwrap();
    assert!(
        builder
            .append_instruction(
                block,
                InstructionKind::Constant(ConstantId(0)),
                &[TypeId(0)]
            )
            .is_err()
    );
    assert!(
        builder
            .set_terminator(block, Terminator::Unreachable)
            .is_err()
    );
    builder.finish(block).unwrap();
    verify_ir(&ir).unwrap();
    assert!(FunctionBuilder::new(&mut ir, FunctionId(0)).is_err());
}

#[test]
fn verifier_rejects_forward_uses_and_non_dominating_values() {
    let mut ir = declaration_ir();
    ir.types.push(IrType::Bool);
    ir.constants.push(Constant {
        ty: TypeId(1),
        kind: ConstantKind::Bool(true),
    });
    let mut builder = FunctionBuilder::new(&mut ir, FunctionId(0)).unwrap();
    let entry = builder.create_block(&[]).unwrap();
    let left = builder.create_block(&[]).unwrap();
    let right = builder.create_block(&[]).unwrap();
    let join = builder.create_block(&[]).unwrap();
    let value = builder
        .append_instruction(left, InstructionKind::Constant(ConstantId(0)), &[TypeId(0)])
        .unwrap()[0];
    builder
        .set_terminator(
            entry,
            Terminator::Branch {
                condition: Operand::Constant(ConstantId(1)),
                then_edge: Edge {
                    target: left,
                    args: vec![],
                },
                else_edge: Edge {
                    target: right,
                    args: vec![],
                },
            },
        )
        .unwrap();
    for block in [left, right] {
        builder
            .set_terminator(
                block,
                Terminator::Jump(Edge {
                    target: join,
                    args: vec![],
                }),
            )
            .unwrap();
    }
    builder
        .set_terminator(
            join,
            Terminator::Return {
                values: vec![Operand::Value(value)],
            },
        )
        .unwrap();
    assert!(
        builder
            .finish(entry)
            .unwrap_err()
            .to_string()
            .contains("dominate")
    );
    assert!(ir.blocks.is_empty());

    let mut ir = sample_ir();
    ir.types.truncate(2);
    let InstructionKind::Call { args, .. } = &mut ir.blocks[1].instructions[0].kind else {
        unreachable!()
    };
    *args = vec![Operand::Value(ValueId(2))];
    assert!(verify_ir(&ir).unwrap_err().to_string().contains("dominate"));
}

#[test]
fn display_is_diagnostic_and_does_not_require_valid_references() {
    let text = sample_ir().to_string();
    assert!(text.contains("unverified"));
    assert!(text.contains("Declaration"));
    assert!(text.contains("bb1 (owner fn0"));
    assert!(text.contains("Branch"));
    assert!(text.contains("Return"));
    assert_eq!(Operand::Value(ValueId(3)).to_string(), "%v3");
    assert_eq!(Operand::Constant(ConstantId(3)).to_string(), "const3");
    let mut invalid = Ir::new();
    invalid.values.push(Value {
        owner: FunctionId(99),
        ty: TypeId(99),
        definition: ValueDefinition::BlockParameter {
            block: BlockId(99),
            index: 0,
        },
    });
    assert!(invalid.to_string().contains("99"));
    assert!(verify_ir(&invalid).is_err());
}

#[test]
fn verifier_accepts_loop_parameters_and_backedge_arguments() {
    let mut ir = declaration_ir();
    ir.types.push(IrType::Bool);
    ir.constants.push(Constant {
        ty: TypeId(1),
        kind: ConstantKind::Bool(false),
    });
    let mut builder = FunctionBuilder::new(&mut ir, FunctionId(0)).unwrap();
    let entry = builder.create_block(&[]).unwrap();
    let header = builder.create_block(&[TypeId(0)]).unwrap();
    let body = builder.create_block(&[]).unwrap();
    let exit = builder.create_block(&[TypeId(0)]).unwrap();
    let parameter = builder.block_params(header).unwrap()[0];
    let result = builder.block_params(exit).unwrap()[0];
    builder
        .set_terminator(
            entry,
            Terminator::Jump(Edge {
                target: header,
                args: vec![Operand::Constant(ConstantId(0))],
            }),
        )
        .unwrap();
    builder
        .set_terminator(
            header,
            Terminator::Branch {
                condition: Operand::Constant(ConstantId(1)),
                then_edge: Edge {
                    target: body,
                    args: vec![],
                },
                else_edge: Edge {
                    target: exit,
                    args: vec![Operand::Value(parameter)],
                },
            },
        )
        .unwrap();
    builder
        .set_terminator(
            body,
            Terminator::Jump(Edge {
                target: header,
                args: vec![Operand::Value(parameter)],
            }),
        )
        .unwrap();
    builder
        .set_terminator(
            exit,
            Terminator::Return {
                values: vec![Operand::Value(result)],
            },
        )
        .unwrap();
    builder.finish(entry).unwrap();
    verify_ir(&ir).unwrap();
}

#[test]
fn comparisons_validate_operands_and_boolean_result() {
    for op in [
        BinaryOp::Eq,
        BinaryOp::Ne,
        BinaryOp::Lt,
        BinaryOp::Le,
        BinaryOp::Gt,
        BinaryOp::Ge,
    ] {
        let mut ir = declaration_ir();
        ir.types.push(IrType::Bool);
        ir.functions[0].signature.results = vec![TypeId(1)];
        let mut b = FunctionBuilder::new(&mut ir, FunctionId(0)).unwrap();
        let entry = b.create_block(&[]).unwrap();
        let value = b
            .append_instruction(
                entry,
                InstructionKind::Binary {
                    op,
                    lhs: Operand::Constant(ConstantId(0)),
                    rhs: Operand::Constant(ConstantId(0)),
                },
                &[TypeId(1)],
            )
            .unwrap()[0];
        b.set_terminator(
            entry,
            Terminator::Return {
                values: vec![Operand::Value(value)],
            },
        )
        .unwrap();
        b.finish(entry).unwrap();
        verify_ir(&ir).unwrap();
        let mut bad = ir.clone();
        bad.values[0].ty = TypeId(0);
        assert!(verify_ir(&bad).is_err());
        let mut bad = ir.clone();
        bad.blocks[0].instructions[0].results.clear();
        assert!(verify_ir(&bad).is_err());
        let mut bad = ir.clone();
        bad.constants.push(Constant {
            ty: TypeId(1),
            kind: ConstantKind::Bool(true),
        });
        let InstructionKind::Binary { rhs, .. } = &mut bad.blocks[0].instructions[0].kind else {
            unreachable!()
        };
        *rhs = Operand::Constant(ConstantId(1));
        assert!(verify_ir(&bad).is_err());
        let mut bad = ir.clone();
        bad.blocks[0].instructions[0].kind = InstructionKind::Binary {
            op: BinaryOp::Add,
            lhs: Operand::Constant(ConstantId(0)),
            rhs: Operand::Constant(ConstantId(0)),
        };
        assert!(verify_ir(&bad).is_err());
    }
    for op in [BinaryOp::Eq, BinaryOp::Ne, BinaryOp::Lt] {
        let mut ir = declaration_ir();
        ir.types.push(IrType::Bool);
        ir.constants.push(Constant {
            ty: TypeId(1),
            kind: ConstantKind::Bool(true),
        });
        ir.functions[0].signature.results = vec![TypeId(1)];
        let mut b = FunctionBuilder::new(&mut ir, FunctionId(0)).unwrap();
        let entry = b.create_block(&[]).unwrap();
        let value = b
            .append_instruction(
                entry,
                InstructionKind::Binary {
                    op,
                    lhs: Operand::Constant(ConstantId(1)),
                    rhs: Operand::Constant(ConstantId(1)),
                },
                &[TypeId(1)],
            )
            .unwrap()[0];
        b.set_terminator(
            entry,
            Terminator::Return {
                values: vec![Operand::Value(value)],
            },
        )
        .unwrap();
        if op == BinaryOp::Lt {
            assert!(b.finish(entry).is_err());
        } else {
            b.finish(entry).unwrap();
        }
    }
}

#[test]
fn numeric_instruction_signatures_and_traps_are_verified() {
    let mut ir = declaration_ir();
    ir.types
        .extend([IrType::Float(FloatWidth::F32), IrType::Bool]);
    ir.constants.extend([
        Constant {
            ty: TypeId(1),
            kind: ConstantKind::Float { bits: 0x3f800000 },
        },
        Constant {
            ty: TypeId(2),
            kind: ConstantKind::Bool(true),
        },
    ]);
    let mut b = FunctionBuilder::new(&mut ir, FunctionId(0)).unwrap();
    let entry = b.create_block(&[]).unwrap();
    let float = b
        .append_instruction(
            entry,
            InstructionKind::Cast {
                kind: CastKind::Numeric,
                operand: Operand::Constant(ConstantId(0)),
            },
            &[TypeId(1)],
        )
        .unwrap()[0];
    let sum = b
        .append_instruction(
            entry,
            InstructionKind::Binary {
                op: BinaryOp::Add,
                lhs: Operand::Value(float),
                rhs: Operand::Constant(ConstantId(1)),
            },
            &[TypeId(1)],
        )
        .unwrap()[0];
    b.append_instruction(
        entry,
        InstructionKind::Unary {
            op: UnaryOp::Neg,
            operand: Operand::Value(sum),
        },
        &[TypeId(1)],
    )
    .unwrap();
    b.set_terminator(entry, Terminator::Trap(TrapReason::InvalidFloatToInteger))
        .unwrap();
    b.finish(entry).unwrap();
    assert!(ir.to_string().contains("InvalidFloatToInteger"));
    for change in 0..6 {
        let mut bad = ir.clone();
        match change {
            0 => bad.constants[1].kind = ConstantKind::Float { bits: 1u128 << 32 },
            1 => {
                bad.blocks[0].instructions[0].kind = InstructionKind::Cast {
                    kind: CastKind::Numeric,
                    operand: Operand::Constant(ConstantId(2)),
                }
            }
            2 => bad.values[float.0].ty = TypeId(2),
            3 => {
                if let InstructionKind::Binary { op, .. } = &mut bad.blocks[0].instructions[1].kind
                {
                    *op = BinaryOp::Rem;
                }
            }
            4 => {
                bad.blocks[0].instructions[2].kind = InstructionKind::Unary {
                    op: UnaryOp::Not,
                    operand: Operand::Value(sum),
                }
            }
            5 => bad.constants[1].kind = ConstantKind::Integer { bits: 1 },
            _ => unreachable!(),
        }
        assert!(verify_ir(&bad).is_err(), "corruption {change}");
    }
}

fn memory_ir() -> Ir {
    let mut ir = Ir::new();
    ir.types = vec![
        IrType::Integer {
            width: IntegerWidth::Bits(32),
            signed: true,
        },
        IrType::Bool,
        IrType::Struct {
            name: Some("Pair".into()),
            fields: vec![TypeId(0), TypeId(1)],
        },
        IrType::Pointer { pointee: TypeId(2) },
        IrType::Pointer { pointee: TypeId(0) },
    ];
    ir.constants = vec![
        Constant {
            ty: TypeId(0),
            kind: ConstantKind::Integer { bits: 42 },
        },
        Constant {
            ty: TypeId(1),
            kind: ConstantKind::Bool(true),
        },
    ];
    ir.functions.push(Function {
        identifier: "memory".into(),
        signature: Signature {
            params: vec![],
            results: vec![TypeId(0)],
            calling_convention: CallingConvention::Fib,
        },
        linkage: Linkage::Internal,
        body: FunctionBody::Declaration,
    });
    let mut b = FunctionBuilder::new(&mut ir, FunctionId(0)).unwrap();
    let entry = b.create_block(&[]).unwrap();
    let slot = b.create_stack_slot(TypeId(2), None).unwrap();
    let pointer = b
        .append_instruction(entry, InstructionKind::StackAddress(slot), &[TypeId(3)])
        .unwrap()[0];
    let field = b
        .append_instruction(
            entry,
            InstructionKind::FieldAddress {
                base: Address(Operand::Value(pointer)),
                field: 0,
            },
            &[TypeId(4)],
        )
        .unwrap()[0];
    let pair = b
        .append_instruction(
            entry,
            InstructionKind::Aggregate {
                elements: vec![
                    Operand::Constant(ConstantId(0)),
                    Operand::Constant(ConstantId(1)),
                ],
            },
            &[TypeId(2)],
        )
        .unwrap()[0];
    b.append_instruction(
        entry,
        InstructionKind::Store {
            address: Address(Operand::Value(pointer)),
            value: Operand::Value(pair),
        },
        &[],
    )
    .unwrap();
    let result = b
        .append_instruction(
            entry,
            InstructionKind::Load {
                address: Address(Operand::Value(field)),
            },
            &[TypeId(0)],
        )
        .unwrap()[0];
    b.set_terminator(
        entry,
        Terminator::Return {
            values: vec![Operand::Value(result)],
        },
    )
    .unwrap();
    b.finish(entry).unwrap();
    ir
}

#[test]
fn memory_verification_checks_slots_projections_and_signatures() {
    let ir = memory_ir();
    verify_ir(&ir).unwrap();
    let mutations: &[fn(&mut Ir)] = &[
        |ir| ir.stack_slots[0].owner = FunctionId(99),
        |ir| ir.stack_slots[0].ty = TypeId(99),
        |ir| ir.stack_slots.push(ir.stack_slots[0].clone()),
        |ir| {
            if let FunctionBody::Definition { stack_slots, .. } = &mut ir.functions[0].body {
                stack_slots.clear();
            }
        },
        |ir| {
            if let FunctionBody::Definition { stack_slots, .. } = &mut ir.functions[0].body {
                stack_slots.push(StackSlotId(0));
            }
        },
        |ir| ir.blocks[0].instructions[0].kind = InstructionKind::StackAddress(StackSlotId(99)),
        |ir| {
            if let InstructionKind::FieldAddress { field, .. } =
                &mut ir.blocks[0].instructions[1].kind
            {
                *field = 2;
            }
        },
        |ir| {
            if let InstructionKind::FieldAddress { base, .. } =
                &mut ir.blocks[0].instructions[1].kind
            {
                base.0 = Operand::Constant(ConstantId(0));
            }
        },
        |ir| {
            if let InstructionKind::Aggregate { elements } = &mut ir.blocks[0].instructions[2].kind
            {
                elements.pop();
            }
        },
        |ir| {
            if let InstructionKind::Store { value, .. } = &mut ir.blocks[0].instructions[3].kind {
                *value = Operand::Constant(ConstantId(0));
            }
        },
        |ir| ir.values[3].ty = TypeId(1),
        |ir| {
            ir.types[3] = IrType::Pointer {
                pointee: TypeId(99),
            }
        },
        |ir| {
            ir.types[2] = IrType::Struct {
                name: None,
                fields: vec![TypeId(2)],
            }
        },
        |ir| ir.blocks[0].instructions.swap(0, 1),
    ];
    for mutate in mutations {
        let mut invalid = ir.clone();
        mutate(&mut invalid);
        assert!(verify_ir(&invalid).is_err());
    }
}

#[test]
fn pointer_recursion_is_sized_but_by_value_cycles_and_bad_nulls_are_rejected() {
    let mut ir = Ir::new();
    ir.types = vec![
        IrType::Struct {
            name: None,
            fields: vec![TypeId(1)],
        },
        IrType::Pointer { pointee: TypeId(0) },
    ];
    ir.constants.push(Constant {
        ty: TypeId(1),
        kind: ConstantKind::Null,
    });
    verify_ir(&ir).unwrap();
    ir.constants[0].ty = TypeId(0);
    assert!(verify_ir(&ir).is_err());
    ir.constants.clear();
    ir.types[1] = IrType::Struct {
        name: None,
        fields: vec![TypeId(0)],
    };
    assert!(
        verify_ir(&ir)
            .unwrap_err()
            .to_string()
            .contains("recursive")
    );
}

#[test]
fn failed_memory_builder_does_not_publish_stack_slots() {
    let mut ir = memory_ir();
    let mut declaration = ir.functions[0].clone();
    declaration.identifier = "bad".into();
    declaration.body = FunctionBody::Declaration;
    ir.functions.push(declaration);
    let before = ir.to_string();
    let mut b = FunctionBuilder::new(&mut ir, FunctionId(1)).unwrap();
    let entry = b.create_block(&[]).unwrap();
    let slot = b.create_stack_slot(TypeId(2), None).unwrap();
    b.append_instruction(entry, InstructionKind::StackAddress(slot), &[TypeId(4)])
        .unwrap();
    b.set_terminator(
        entry,
        Terminator::Return {
            values: vec![Operand::Constant(ConstantId(0))],
        },
    )
    .unwrap();
    assert!(b.finish(entry).is_err());
    assert_eq!(before, ir.to_string());
}

fn enum_ir() -> Ir {
    let mut ir = Ir::new();
    ir.types = vec![
        IrType::Integer {
            width: IntegerWidth::Bits(32),
            signed: false,
        },
        IrType::Bool,
        IrType::Enum {
            name: Some("E".into()),
            variants: vec![
                EnumVariant {
                    name: Some("Flag".into()),
                    discriminant: 7,
                    fields: vec![TypeId(1)],
                },
                EnumVariant {
                    name: Some("Value".into()),
                    discriminant: 42,
                    fields: vec![TypeId(0)],
                },
            ],
        },
    ];
    ir.constants = vec![
        Constant {
            ty: TypeId(0),
            kind: ConstantKind::Integer { bits: 42 },
        },
        Constant {
            ty: TypeId(0),
            kind: ConstantKind::Integer { bits: 7 },
        },
    ];
    ir.functions.push(Function {
        identifier: "read".into(),
        signature: Signature {
            params: vec![TypeId(2)],
            results: vec![TypeId(0)],
            calling_convention: CallingConvention::Fib,
        },
        linkage: Linkage::Internal,
        body: FunctionBody::Declaration,
    });
    let mut b = FunctionBuilder::new(&mut ir, FunctionId(0)).unwrap();
    let entry = b.create_block(&[TypeId(2)]).unwrap();
    let value = Operand::Value(b.block_params(entry).unwrap()[0]);
    let arm = b.create_block(&[]).unwrap();
    let default = b.create_block(&[]).unwrap();
    let tag = b
        .append_instruction(entry, InstructionKind::EnumTag { value }, &[TypeId(0)])
        .unwrap()[0];
    b.set_terminator(
        entry,
        Terminator::Switch {
            subject: Operand::Value(tag),
            cases: vec![SwitchCase {
                value: ConstantId(0),
                edge: Edge {
                    target: arm,
                    args: vec![],
                },
            }],
            default: Edge {
                target: default,
                args: vec![],
            },
        },
    )
    .unwrap();
    let field = b
        .append_instruction(
            arm,
            InstructionKind::EnumField {
                value,
                variant: 1,
                field: 0,
            },
            &[TypeId(0)],
        )
        .unwrap()[0];
    b.set_terminator(
        arm,
        Terminator::Return {
            values: vec![Operand::Value(field)],
        },
    )
    .unwrap();
    b.set_terminator(
        default,
        Terminator::Return {
            values: vec![Operand::Constant(ConstantId(1))],
        },
    )
    .unwrap();
    b.finish(entry).unwrap();
    ir
}

#[test]
fn enums_verify_payloads_tags_switch_edges_and_active_variant_proofs() {
    let ir = enum_ir();
    verify_ir(&ir).unwrap();
    let mutations: &[fn(&mut Ir)] = &[
        |ir| {
            if let IrType::Enum { variants, .. } = &mut ir.types[2] {
                variants[1].discriminant = 7;
            }
        },
        |ir| {
            if let IrType::Enum { variants, .. } = &mut ir.types[2] {
                variants[1].fields[0] = TypeId(99);
            }
        },
        |ir| {
            if let IrType::Enum { variants, .. } = &mut ir.types[2] {
                variants[1].fields[0] = TypeId(2);
            }
        },
        |ir| {
            if let IrType::Enum { variants, .. } = &mut ir.types[2] {
                variants[1].discriminant = u128::MAX;
            }
        },
        |ir| ir.values[1].ty = TypeId(1),
        |ir| {
            if let Terminator::Switch { subject, .. } = &mut ir.blocks[0].terminator {
                *subject = Operand::Constant(ConstantId(0));
            }
        },
        |ir| {
            if let Terminator::Switch { cases, .. } = &mut ir.blocks[0].terminator {
                cases.push(cases[0].clone());
            }
        },
        |ir| {
            if let Terminator::Switch { cases, .. } = &mut ir.blocks[0].terminator {
                cases[0].value = ConstantId(99);
            }
        },
        |ir| {
            if let Terminator::Switch { cases, .. } = &mut ir.blocks[0].terminator {
                cases[0].edge.args.push(Operand::Constant(ConstantId(0)));
            }
        },
        |ir| {
            if let Terminator::Switch { default, .. } = &mut ir.blocks[0].terminator {
                default.target = BlockId(1);
            }
        },
        |ir| {
            if let InstructionKind::EnumField { variant, .. } =
                &mut ir.blocks[1].instructions[0].kind
            {
                *variant = 0;
            }
        },
        |ir| {
            if let InstructionKind::EnumField { field, .. } = &mut ir.blocks[1].instructions[0].kind
            {
                *field = 1;
            }
        },
        |ir| ir.blocks[1].instructions[0].results.clear(),
    ];
    for (index, mutate) in mutations.iter().enumerate() {
        let mut bad = ir.clone();
        mutate(&mut bad);
        assert!(verify_ir(&bad).is_err(), "mutation {index}");
    }
}

#[test]
fn enum_construction_and_constant_graphs_have_exact_signatures() {
    let mut ir = enum_ir();
    // An enum constant with forward-referenced aggregate payload.
    ir.types.push(IrType::Struct {
        name: None,
        fields: vec![TypeId(0)],
    });
    ir.types.push(IrType::Enum {
        name: None,
        variants: vec![EnumVariant {
            name: None,
            discriminant: u128::MAX,
            fields: vec![TypeId(3)],
        }],
    });
    ir.constants.push(Constant {
        ty: TypeId(4),
        kind: ConstantKind::Enum {
            variant: 0,
            fields: vec![ConstantId(3)],
        },
    });
    ir.constants.push(Constant {
        ty: TypeId(3),
        kind: ConstantKind::Aggregate(vec![ConstantId(0)]),
    });
    verify_ir(&ir).unwrap();
    for mutate in [
        |ir: &mut Ir| {
            if let ConstantKind::Enum { variant, .. } = &mut ir.constants[2].kind {
                *variant = 1;
            }
        },
        |ir: &mut Ir| {
            if let ConstantKind::Enum { fields, .. } = &mut ir.constants[2].kind {
                fields[0] = ConstantId(0);
            }
        },
        |ir: &mut Ir| {
            if let ConstantKind::Aggregate(fields) = &mut ir.constants[3].kind {
                fields[0] = ConstantId(99);
            }
        },
        |ir: &mut Ir| {
            if let ConstantKind::Aggregate(fields) = &mut ir.constants[3].kind {
                fields[0] = ConstantId(3);
            }
        },
    ] {
        let mut bad = ir.clone();
        mutate(&mut bad);
        assert!(verify_ir(&bad).is_err());
    }
    // Construction establishes the extraction precondition without a switch.
    ir.functions.push(Function {
        identifier: "construct".into(),
        signature: Signature {
            params: vec![],
            results: vec![TypeId(0)],
            calling_convention: CallingConvention::Fib,
        },
        linkage: Linkage::Internal,
        body: FunctionBody::Declaration,
    });
    let mut b = FunctionBuilder::new(&mut ir, FunctionId(1)).unwrap();
    let entry = b.create_block(&[]).unwrap();
    let value = b
        .append_instruction(
            entry,
            InstructionKind::Enum {
                variant: 1,
                fields: vec![Operand::Constant(ConstantId(0))],
            },
            &[TypeId(2)],
        )
        .unwrap()[0];
    let result = b
        .append_instruction(
            entry,
            InstructionKind::EnumField {
                value: Operand::Value(value),
                variant: 1,
                field: 0,
            },
            &[TypeId(0)],
        )
        .unwrap()[0];
    b.set_terminator(
        entry,
        Terminator::Return {
            values: vec![Operand::Value(result)],
        },
    )
    .unwrap();
    b.finish(entry).unwrap();
    let mut bad = ir.clone();
    if let InstructionKind::Enum { fields, .. } = &mut bad.blocks[3].instructions[0].kind {
        fields.clear();
    }
    assert!(verify_ir(&bad).is_err());
}

#[test]
fn enum_pointer_recursion_is_finite() {
    let mut ir = Ir::new();
    ir.types = vec![
        IrType::Enum {
            name: None,
            variants: vec![EnumVariant {
                name: None,
                discriminant: 0,
                fields: vec![TypeId(1)],
            }],
        },
        IrType::Pointer { pointee: TypeId(0) },
    ];
    verify_ir(&ir).unwrap();
    ir.types[1] = IrType::Struct {
        name: None,
        fields: vec![TypeId(0)],
    };
    assert!(
        verify_ir(&ir)
            .unwrap_err()
            .to_string()
            .contains("recursive")
    );
}
