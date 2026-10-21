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
                name: "example".into(),
                signature: signature(),
                linkage: Linkage::Internal,
                body: FunctionBody::Definition {
                    entry: BlockId(0),
                    blocks: vec![BlockId(0), BlockId(1)],
                    stack_slots: vec![],
                },
            },
            Function {
                name: "external".into(),
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
    assert_eq!(ir.block(BlockId(1)).unwrap().owner, FunctionId(0));
    assert_eq!(ir.value(ValueId(2)).unwrap().owner, FunctionId(0));
    assert_eq!(ir.operand_type(Operand::Value(ValueId(2))), Some(TypeId(0)));
    assert_eq!(
        ir.operand_type(Operand::Constant(ConstantId(0))),
        Some(TypeId(1))
    );
    assert!(matches!(
        ir.ty(TypeId(0)),
        Some(IrType::Integer { signed: true, .. })
    ));
    assert!(ir.function(FunctionId(99)).is_none());
    assert!(ir.global(GlobalId(99)).is_none());
    assert!(ir.ty(TypeId(99)).is_none());
    assert!(ir.constant(ConstantId(99)).is_none());
    assert!(ir.block(BlockId(99)).is_none());
    assert!(ir.value(ValueId(99)).is_none());
    assert!(ir.stack_slot(StackSlotId(99)).is_none());
    assert_eq!(ir.operand_type(Operand::Value(ValueId(99))), None);
}

#[test]
fn declarations_retain_signatures_without_blocks() {
    let mut ir = Ir::new();
    assert!(ir.is_empty());
    ir.functions.push(sample_ir().functions.remove(1));
    assert!(!ir.is_empty());
    assert!(ir.blocks.is_empty());
    let function = ir.function(FunctionId(0)).unwrap();
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
        |ir| ir.functions[1].name = "example".into(),
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
        name: "f".into(),
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
