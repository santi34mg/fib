use crate::ir::{Function, FunctionBody, Ir, Linkage, Signature};

use super::{BackendError, TargetSpec, emit_llvm, ir_lower::verify_input};

fn target() -> TargetSpec {
    TargetSpec {
        triple: "x86_64-unknown-linux-gnu".into(),
        cpu: "generic".into(),
        features: Vec::new(),
    }
}

#[test]
fn malformed_ir_is_rejected_before_emission() {
    let mut ir = Ir::new();
    ir.functions.push(Function {
        identifier: "bad".into(),
        signature: Signature {
            params: vec![crate::ir::TypeId(99)],
            results: vec![],
            calling_convention: crate::ir::CallingConvention::C,
        },
        linkage: Linkage::External,
        body: FunctionBody::Declaration,
    });
    assert!(matches!(
        emit_llvm(&ir, &target()),
        Err(BackendError::Invalid { .. })
    ));
    assert!(verify_input(&ir).is_err());
}

#[test]
fn input_verification_runs_real_verifier() {
    let ir = Ir::new();
    assert!(verify_input(&ir).is_ok());
}

#[cfg(feature = "llvm")]
#[test]
fn target_validation_and_llvm_verification_run() {
    let ir = Ir::new();
    let artifact = emit_llvm(&ir, &target()).unwrap();
    assert!(artifact.ir_text().contains("target datalayout"));
    assert!(!artifact.data_layout().is_empty());
    let mut spec = target();
    spec.triple = "invalid-target".into();
    assert!(emit_llvm(&ir, &spec).is_err());
    spec = target();
    spec.features.push("unknown".into());
    assert!(emit_llvm(&ir, &spec).is_err());
}

#[test]
fn errors_identify_the_unfinished_operation() {
    let error = BackendError::NotImplemented {
        operation: "unsupported_operation",
    };
    let as_error: &dyn std::error::Error = &error;
    assert!(as_error.to_string().contains("unsupported_operation"));
    assert!(as_error.to_string().contains("not implemented"));
}

#[cfg(feature = "llvm")]
#[test]
fn scalar_calls_preserve_parameters_results_and_void_returns() {
    use crate::ir::*;
    let mut ir = Ir::new();
    ir.types.push(IrType::Integer {
        width: IntegerWidth::Bits(32),
        signed: true,
    });
    for (name, params, results) in [
        ("identity", vec![TypeId(0)], vec![TypeId(0)]),
        ("effect", vec![], vec![]),
        ("main", vec![], vec![TypeId(0)]),
    ] {
        ir.functions.push(Function {
            identifier: name.into(),
            signature: Signature {
                params,
                results,
                calling_convention: CallingConvention::C,
            },
            linkage: Linkage::External,
            body: FunctionBody::Declaration,
        });
    }
    ir.constants.push(Constant {
        ty: TypeId(0),
        kind: ConstantKind::Integer { bits: 42 },
    });
    let mut builder = FunctionBuilder::new(&mut ir, FunctionId(0)).unwrap();
    let entry = builder.create_block(&[TypeId(0)]).unwrap();
    let arg = builder.block_params(entry).unwrap()[0];
    builder
        .set_terminator(
            entry,
            Terminator::Return {
                values: vec![Operand::Value(arg)],
            },
        )
        .unwrap();
    builder.finish(entry).unwrap();
    let mut builder = FunctionBuilder::new(&mut ir, FunctionId(1)).unwrap();
    let entry = builder.create_block(&[]).unwrap();
    builder
        .set_terminator(entry, Terminator::Return { values: vec![] })
        .unwrap();
    builder.finish(entry).unwrap();
    let mut builder = FunctionBuilder::new(&mut ir, FunctionId(2)).unwrap();
    let entry = builder.create_block(&[]).unwrap();
    builder
        .append_instruction(
            entry,
            InstructionKind::Call {
                callee: FunctionId(1),
                args: vec![],
            },
            &[],
        )
        .unwrap();
    let value = builder
        .append_instruction(
            entry,
            InstructionKind::Call {
                callee: FunctionId(0),
                args: vec![Operand::Constant(ConstantId(0))],
            },
            &[TypeId(0)],
        )
        .unwrap()[0];
    builder
        .set_terminator(
            entry,
            Terminator::Return {
                values: vec![Operand::Value(value)],
            },
        )
        .unwrap();
    builder.finish(entry).unwrap();
    let artifact = emit_llvm(&ir, &target()).unwrap();
    assert!(artifact.ir_text().contains("call i32 @identity(i32 42)"));
    assert!(artifact.ir_text().contains("call void @effect()"));
    ir.functions[0].signature.calling_convention = CallingConvention::Fib;
    let artifact = emit_llvm(&ir, &target()).unwrap();
    assert!(
        artifact
            .ir_text()
            .contains("call fastcc i32 @identity(i32 42)")
    );
}
