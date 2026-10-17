//! Structural contract tests only: no frontend/lowering integration tests.

use super::*;

fn signature() -> Signature {
    Signature {
        params: vec![TypeId(0)],
        results: vec![TypeId(0)],
        variadic: false,
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
fn unfinished_apis_error_even_on_empty_ir_without_mutation() {
    let mut ir = Ir::new();
    assert_eq!(
        verify_ir(&ir),
        Err(IrError::NotImplemented {
            operation: "verify_ir"
        })
    );
    assert_eq!(
        verify_ir(&sample_ir()),
        Err(IrError::NotImplemented {
            operation: "verify_ir"
        })
    );
    let error = FunctionBuilder::new(&mut ir, FunctionId(0)).unwrap_err();
    assert_eq!(
        error,
        IrError::NotImplemented {
            operation: "FunctionBuilder::new"
        }
    );
    assert!(error.to_string().contains("not implemented"));
    assert!(ir.is_empty());
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
