//! Executable hand-built CFGs isolate backend behavior from source lowering.
use super::{
    emit_llvm,
    llvm_lower::{emit_classified, host_target},
};
use crate::ir::*;

fn module() -> Ir {
    let mut ir = Ir::new();
    ir.types = vec![
        IrType::Integer {
            width: IntegerWidth::Bits(32),
            signed: true,
        },
        IrType::Bool,
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
        Constant {
            ty: TypeId(1),
            kind: ConstantKind::Bool(true),
        },
        Constant {
            ty: TypeId(1),
            kind: ConstantKind::Bool(false),
        },
    ];
    ir.functions.push(Function {
        name: "main".into(),
        signature: Signature {
            params: vec![],
            results: vec![TypeId(0)],
            variadic: false,
            calling_convention: CallingConvention::C,
        },
        linkage: Linkage::External,
        body: FunctionBody::Declaration,
    });
    ir
}
fn c(id: usize) -> Operand {
    Operand::Constant(ConstantId(id))
}
fn run(artifact: super::LlvmArtifact) -> i32 {
    let directory = tempfile::tempdir().unwrap();
    let object = directory.path().join("program.o");
    let binary = directory.path().join("program");
    artifact.write_object(&object).unwrap();
    let output = std::process::Command::new("cc")
        .arg(object)
        .arg("-o")
        .arg(&binary)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::process::Command::new(binary)
        .status()
        .unwrap()
        .code()
        .unwrap()
}
#[test]
fn parallel_edges_with_distinct_arguments_use_distinct_predecessors() {
    for (condition, expected) in [(2, 42), (3, 7)] {
        let mut ir = module();
        let mut b = FunctionBuilder::new(&mut ir, FunctionId(0)).unwrap();
        let entry = b.create_block(&[]).unwrap();
        let join = b.create_block(&[TypeId(0)]).unwrap();
        let result = b.block_params(join).unwrap()[0];
        b.set_terminator(
            entry,
            Terminator::Branch {
                condition: c(condition),
                then_edge: Edge {
                    target: join,
                    args: vec![c(0)],
                },
                else_edge: Edge {
                    target: join,
                    args: vec![c(1)],
                },
            },
        )
        .unwrap();
        b.set_terminator(
            join,
            Terminator::Return {
                values: vec![Operand::Value(result)],
            },
        )
        .unwrap();
        b.finish(entry).unwrap();
        let artifact = emit_llvm(&ir, &host_target().unwrap()).unwrap();
        assert!(artifact.ir_text().contains("phi i32"));
        assert_eq!(run(artifact), expected);
    }
}
#[test]
fn backedges_multiple_parameters_and_shuffled_blocks_execute() {
    let mut ir = module();
    let mut b = FunctionBuilder::new(&mut ir, FunctionId(0)).unwrap();
    let entry = b.create_block(&[]).unwrap();
    let header = b.create_block(&[TypeId(0), TypeId(1)]).unwrap();
    let body = b.create_block(&[]).unwrap();
    let exit = b.create_block(&[TypeId(0)]).unwrap();
    let args = b.block_params(header).unwrap().to_vec();
    let result = b.block_params(exit).unwrap()[0];
    b.set_terminator(
        entry,
        Terminator::Jump(Edge {
            target: header,
            args: vec![c(1), c(2)],
        }),
    )
    .unwrap();
    b.set_terminator(
        header,
        Terminator::Branch {
            condition: Operand::Value(args[1]),
            then_edge: Edge {
                target: body,
                args: vec![],
            },
            else_edge: Edge {
                target: exit,
                args: vec![Operand::Value(args[0])],
            },
        },
    )
    .unwrap();
    // Use a real instruction result as a forward-referenced backedge argument.
    let next = b
        .append_instruction(body, InstructionKind::Constant(ConstantId(0)), &[TypeId(0)])
        .unwrap()[0];
    b.set_terminator(
        body,
        Terminator::Jump(Edge {
            target: header,
            args: vec![Operand::Value(next), c(3)],
        }),
    )
    .unwrap();
    b.set_terminator(
        exit,
        Terminator::Return {
            values: vec![Operand::Value(result)],
        },
    )
    .unwrap();
    b.finish(entry).unwrap();
    let FunctionBody::Definition { blocks, .. } = &mut ir.functions[0].body else {
        unreachable!()
    };
    blocks.reverse();
    let artifact = emit_llvm(&ir, &host_target().unwrap()).unwrap();
    assert!(artifact.ir_text().contains("phi i1"));
    assert_eq!(run(artifact), 42);
}
#[test]
fn each_functions_abi_controls_its_calls_and_returns() {
    use super::abi::{classify_signature, classify_with};
    let mut ir = module();
    ir.functions.push(Function {
        name: "test_fib".into(),
        signature: Signature {
            params: vec![TypeId(0), TypeId(0)],
            results: vec![TypeId(0)],
            variadic: false,
            calling_convention: CallingConvention::Fib,
        },
        linkage: Linkage::Internal,
        body: FunctionBody::Declaration,
    });
    let mut b = FunctionBuilder::new(&mut ir, FunctionId(1)).unwrap();
    let entry = b.create_block(&[TypeId(0), TypeId(0)]).unwrap();
    let first = b.block_params(entry).unwrap()[0];
    b.set_terminator(
        entry,
        Terminator::Return {
            values: vec![Operand::Value(first)],
        },
    )
    .unwrap();
    b.finish(entry).unwrap();
    let mut b = FunctionBuilder::new(&mut ir, FunctionId(0)).unwrap();
    let entry = b.create_block(&[]).unwrap();
    let result = b
        .append_instruction(
            entry,
            InstructionKind::Call {
                callee: FunctionId(1),
                args: vec![c(0), c(1)],
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
    let target = host_target().unwrap();
    assert!(emit_llvm(&ir, &target).is_err());
    let artifact = emit_classified(&ir, &target, |ir, target| {
        classify_with(ir, target, |signature, types, target| {
            let mut c_signature = signature.clone();
            c_signature.calling_convention = CallingConvention::C;
            let mut abi = classify_signature(&c_signature, types, target)?;
            // Test-only Fib policy, not a production definition of Fib ABI.
            if signature.calling_convention == CallingConvention::Fib {
                abi.parameter_positions.reverse();
            }
            Ok(abi)
        })
    })
    .unwrap();
    assert!(artifact.ir_text().contains("@test_fib(i32 7, i32 42)"));
    assert_eq!(run(artifact), 42);
}

#[test]
fn comparisons_use_operand_signedness_and_boolean_types() {
    for (signed, op, expected) in [
        (true, BinaryOp::Lt, 42),
        (false, BinaryOp::Lt, 7),
        (true, BinaryOp::Le, 42),
        (true, BinaryOp::Gt, 7),
        (true, BinaryOp::Ge, 7),
        (true, BinaryOp::Eq, 7),
        (true, BinaryOp::Ne, 42),
    ] {
        let mut ir = module();
        ir.types[0] = IrType::Integer {
            width: IntegerWidth::Bits(32),
            signed,
        };
        ir.constants.push(Constant {
            ty: TypeId(0),
            kind: ConstantKind::Integer {
                bits: u32::MAX as u128,
            },
        });
        let mut b = FunctionBuilder::new(&mut ir, FunctionId(0)).unwrap();
        let entry = b.create_block(&[]).unwrap();
        let yes = b.create_block(&[]).unwrap();
        let no = b.create_block(&[]).unwrap();
        let condition = b
            .append_instruction(
                entry,
                InstructionKind::Binary {
                    op,
                    lhs: c(4),
                    rhs: c(1),
                },
                &[TypeId(1)],
            )
            .unwrap()[0];
        b.set_terminator(
            entry,
            Terminator::Branch {
                condition: Operand::Value(condition),
                then_edge: Edge {
                    target: yes,
                    args: vec![],
                },
                else_edge: Edge {
                    target: no,
                    args: vec![],
                },
            },
        )
        .unwrap();
        for (block, result) in [(yes, c(0)), (no, c(1))] {
            b.set_terminator(
                block,
                Terminator::Return {
                    values: vec![result],
                },
            )
            .unwrap();
        }
        b.finish(entry).unwrap();
        assert_eq!(
            run(emit_llvm(&ir, &host_target().unwrap()).unwrap()),
            expected
        );
    }
}
