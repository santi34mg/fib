//! Verified scalar SSA emission with blocks/PHIs and per-function ABI adaptation.
use super::{BackendError, TargetSpec};
use crate::ir::Ir;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlvmArtifact {
    ir_text: String,
    target_triple: String,
    data_layout: String,
    target: TargetSpec,
}
impl LlvmArtifact {
    pub fn ir_text(&self) -> &str {
        &self.ir_text
    }
    pub fn target_triple(&self) -> &str {
        &self.target_triple
    }
    pub fn data_layout(&self) -> &str {
        &self.data_layout
    }
    pub fn into_ir_text(self) -> String {
        self.ir_text
    }

    pub fn write_object(&self, path: &std::path::Path) -> Result<(), BackendError> {
        #[cfg(feature = "llvm")]
        {
            let context = inkwell::context::Context::create();
            let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range_copy(
                self.ir_text.as_bytes(),
                "fib",
            );
            let module = context.create_module_from_ir(buffer).map_err(error)?;
            module.verify().map_err(error)?;
            machine(&self.target)?
                .write_to_file(&module, inkwell::targets::FileType::Object, path)
                .map_err(error)
        }
        #[cfg(not(feature = "llvm"))]
        {
            let _ = path;
            Err(BackendError::NotImplemented {
                operation: "object emission requires the llvm feature",
            })
        }
    }
}

#[cfg(feature = "llvm")]
fn error(message: impl ToString) -> BackendError {
    BackendError::Invalid {
        message: message.to_string(),
    }
}

pub fn host_target() -> Result<TargetSpec, BackendError> {
    #[cfg(feature = "llvm")]
    {
        Ok(TargetSpec {
            triple: inkwell::targets::TargetMachine::get_default_triple()
                .as_str()
                .to_string_lossy()
                .into_owned(),
            cpu: "generic".into(),
            features: vec![],
        })
    }
    #[cfg(not(feature = "llvm"))]
    {
        Err(BackendError::NotImplemented {
            operation: "target selection requires the llvm feature",
        })
    }
}

#[cfg(feature = "llvm")]
fn machine(spec: &TargetSpec) -> Result<inkwell::targets::TargetMachine, BackendError> {
    use inkwell::targets::*;
    // Feature validation is intentionally deferred; do not silently accept unknown spellings.
    if spec.cpu != "generic" || !spec.features.is_empty() {
        return Err(error("only generic CPU with default features is supported"));
    }
    Target::initialize_all(&InitializationConfig::default());
    let triple = TargetTriple::create(&spec.triple);
    Target::from_triple(&triple)
        .map_err(error)?
        .create_target_machine(
            &triple,
            &spec.cpu,
            "",
            inkwell::OptimizationLevel::None,
            RelocMode::PIC,
            CodeModel::Default,
        )
        .ok_or_else(|| error("cannot create target machine"))
}

pub fn emit_llvm(ir: &Ir, target: &TargetSpec) -> Result<LlvmArtifact, BackendError> {
    super::ir_lower::verify_input(ir)?;
    #[cfg(feature = "llvm")]
    {
        emit(ir, target)
    }
    #[cfg(not(feature = "llvm"))]
    {
        let _ = target;
        Err(BackendError::NotImplemented {
            operation: "emit_llvm requires the llvm feature",
        })
    }
}

#[cfg(feature = "llvm")]
fn emit(ir: &Ir, target: &TargetSpec) -> Result<LlvmArtifact, BackendError> {
    emit_classified(ir, target, super::abi::classify_abis)
}

#[cfg(feature = "llvm")]
pub(super) fn emit_classified(
    ir: &Ir,
    target: &TargetSpec,
    classify: impl FnOnce(&Ir, &super::abi::AbiTarget) -> Result<super::abi::AbiPlan, BackendError>,
) -> Result<LlvmArtifact, BackendError> {
    super::ir_lower::verify_input(ir)?;
    use crate::ir::*;
    use inkwell::{context::Context, types::BasicTypeEnum, values::BasicValueEnum};
    let context = Context::create();
    let module = context.create_module("fib");
    let machine = machine(target)?;
    module.set_triple(&machine.get_triple());
    module.set_data_layout(&machine.get_target_data().get_data_layout());
    let types: Vec<_> = ir
        .types
        .iter()
        .map(|ty| match ty {
            IrType::Bool => Ok(BasicTypeEnum::from(context.bool_type())),
            IrType::Integer {
                width: IntegerWidth::Bits(width),
                ..
            } => Ok(context.custom_width_int_type(u32::from(*width)).into()),
            IrType::Float(width) => Ok(match width {
                FloatWidth::F16 => context.f16_type(),
                FloatWidth::F32 => context.f32_type(),
                FloatWidth::F64 => context.f64_type(),
                FloatWidth::F128 => context.f128_type(),
            }
            .into()),
            _ => Err(error("unsupported LLVM type")),
        })
        .collect::<Result<_, _>>()?;
    let abi_target = super::abi::AbiTarget {
        triple: machine.get_triple().as_str().to_string_lossy().into_owned(),
        data_layout: module
            .get_data_layout()
            .as_str()
            .to_string_lossy()
            .into_owned(),
    };
    let plan = classify(ir, &abi_target)?;
    let abi = super::llvm_abi::LlvmAbi {
        context: &context,
        plan: &plan,
        types: &types,
    };
    let functions = ir
        .functions
        .iter()
        .enumerate()
        .map(|(i, f)| abi.declare(&module, FunctionId(i), f))
        .collect::<Result<Vec<_>, _>>()?;
    let constants: Vec<BasicValueEnum<'_>> = ir
        .constants
        .iter()
        .map(|c| match c.kind {
            ConstantKind::Bool(b) => Ok(types[c.ty.0]
                .into_int_type()
                .const_int(u64::from(b), false)
                .into()),
            ConstantKind::Integer { bits } => Ok(types[c.ty.0]
                .into_int_type()
                .const_int_arbitrary_precision(&[bits as u64, (bits >> 64) as u64])
                .into()),
            ConstantKind::Float { bits } => Ok(super::llvm_scalar::float_constant(
                types[c.ty.0].into_float_type(),
                bits,
            )
            .into()),
            _ => Err(error("unsupported LLVM constant")),
        })
        .collect::<Result<_, BackendError>>()?;
    let builder = context.create_builder();
    for (fi, f) in ir.functions.iter().enumerate() {
        let FunctionBody::Definition { entry, blocks, .. } = &f.body else {
            continue;
        };
        let function_id = FunctionId(fi);
        let mut llvm_blocks = std::collections::HashMap::new();
        // LLVM treats its first block as entry, regardless of arena/body order.
        for bid in std::iter::once(entry).chain(blocks.iter().filter(|bid| *bid != entry)) {
            llvm_blocks.insert(
                *bid,
                context.append_basic_block(functions[fi], &format!("bb{}", bid.0)),
            );
        }
        let mut values = std::collections::HashMap::new();
        for (&id, arg) in ir.blocks[entry.0]
            .params
            .iter()
            .zip(abi.arguments(function_id, functions[fi])?)
        {
            values.insert(id, arg);
        }
        let mut phis = std::collections::HashMap::new();
        for &bid in blocks {
            if bid == *entry {
                continue;
            }
            builder.position_at_end(llvm_blocks[&bid]);
            for &parameter in &ir.blocks[bid.0].params {
                let phi = builder
                    .build_phi(
                        types[ir.values[parameter.0].ty.0],
                        &format!("v{}", parameter.0),
                    )
                    .map_err(error)?;
                values.insert(parameter, phi.as_basic_value());
                phis.insert(parameter, phi);
            }
        }
        let mut incoming = Vec::new();
        for bid in reverse_postorder(ir, *entry)? {
            let block = &ir.blocks[bid.0];
            let predecessor = llvm_blocks[&bid];
            builder.position_at_end(predecessor);
            for inst in &block.instructions {
                match &inst.kind {
                    InstructionKind::Constant(id) => {
                        values.insert(inst.results[0], constants[id.0]);
                    }
                    InstructionKind::Binary { op, lhs, rhs } => {
                        let ty = ir
                            .operand_type(*lhs)
                            .ok_or_else(|| error("missing scalar operand type"))?;
                        let rhs_ty = ir
                            .operand_type(*rhs)
                            .ok_or_else(|| error("missing scalar operand type"))?;
                        let value = super::llvm_scalar::binary(
                            &builder,
                            &module,
                            *op,
                            &ir.types[ty.0],
                            &ir.types[rhs_ty.0],
                            operand(*lhs, &constants, &values)?,
                            operand(*rhs, &constants, &values)?,
                        )?;
                        values.insert(inst.results[0], value);
                    }
                    InstructionKind::Unary { op, operand: value } => {
                        let value = super::llvm_scalar::unary(
                            &builder,
                            *op,
                            operand(*value, &constants, &values)?,
                        )?;
                        values.insert(inst.results[0], value);
                    }
                    InstructionKind::Cast { operand: value, .. } => {
                        let source_ty = ir
                            .operand_type(*value)
                            .ok_or_else(|| error("missing cast source type"))?;
                        let dest = ir.values[inst.results[0].0].ty;
                        let value = super::llvm_scalar::cast(
                            &builder,
                            &module,
                            &ir.types[source_ty.0],
                            &ir.types[dest.0],
                            types[dest.0],
                            operand(*value, &constants, &values)?,
                        )?;
                        values.insert(inst.results[0], value);
                    }
                    InstructionKind::Call { callee, args } => {
                        let args = args
                            .iter()
                            .map(|&a| operand(a, &constants, &values))
                            .collect::<Result<Vec<_>, _>>()?;
                        if let Some(value) =
                            abi.call(&builder, *callee, functions[callee.0], &args)?
                        {
                            values.insert(inst.results[0], value);
                        }
                    }
                    _ => return Err(error("unsupported LLVM instruction")),
                }
            }
            // Scalar guards may split an LLVM block; PHIs need its final predecessor.
            let predecessor = builder
                .get_insert_block()
                .ok_or_else(|| error("missing insertion block"))?;
            match &block.terminator {
                Terminator::Return { values: result } => {
                    let value = result
                        .first()
                        .map(|&v| operand(v, &constants, &values))
                        .transpose()?;
                    abi.return_value(&builder, function_id, value)?;
                }
                Terminator::Trap(reason) => super::llvm_scalar::trap(&builder, &module, *reason)?,
                Terminator::Unreachable => {
                    builder.build_unreachable().map_err(error)?;
                }
                Terminator::Jump(edge) => {
                    builder
                        .build_unconditional_branch(llvm_blocks[&edge.target])
                        .map_err(error)?;
                    incoming.push((edge, predecessor));
                }
                Terminator::Branch {
                    condition,
                    then_edge,
                    else_edge,
                } => {
                    let condition = operand(*condition, &constants, &values)?.into_int_value();
                    if then_edge.target == else_edge.target {
                        // Each successor occurrence needs its own LLVM predecessor
                        // when the two arms supply different values to one PHI.
                        let then_block = context.append_basic_block(functions[fi], "then.edge");
                        let else_block = context.append_basic_block(functions[fi], "else.edge");
                        builder
                            .build_conditional_branch(condition, then_block, else_block)
                            .map_err(error)?;
                        for (edge, trampoline) in [(then_edge, then_block), (else_edge, else_block)]
                        {
                            builder.position_at_end(trampoline);
                            builder
                                .build_unconditional_branch(llvm_blocks[&edge.target])
                                .map_err(error)?;
                            incoming.push((edge, trampoline));
                        }
                    } else {
                        builder
                            .build_conditional_branch(
                                condition,
                                llvm_blocks[&then_edge.target],
                                llvm_blocks[&else_edge.target],
                            )
                            .map_err(error)?;
                        incoming.push((then_edge, predecessor));
                        incoming.push((else_edge, predecessor));
                    }
                }
                _ => return Err(error("unsupported LLVM terminator")),
            }
        }
        // Backedge arguments may be instruction results emitted after the PHI.
        for (edge, predecessor) in incoming {
            for (&parameter, &argument) in ir.blocks[edge.target.0].params.iter().zip(&edge.args) {
                let value = operand(argument, &constants, &values)?;
                phis.get(&parameter)
                    .ok_or_else(|| error("missing block parameter PHI"))?
                    .add_incoming(&[(&value, predecessor)]);
            }
        }
    }
    module.verify().map_err(error)?;
    Ok(LlvmArtifact {
        ir_text: module.print_to_string().to_string(),
        target_triple: target.triple.clone(),
        data_layout: module
            .get_data_layout()
            .as_str()
            .to_string_lossy()
            .into_owned(),
        target: target.clone(),
    })
}

#[cfg(feature = "llvm")]
fn operand<'ctx>(
    op: crate::ir::Operand,
    constants: &[inkwell::values::BasicValueEnum<'ctx>],
    values: &std::collections::HashMap<crate::ir::ValueId, inkwell::values::BasicValueEnum<'ctx>>,
) -> Result<inkwell::values::BasicValueEnum<'ctx>, BackendError> {
    match op {
        crate::ir::Operand::Constant(id) => constants
            .get(id.0)
            .copied()
            .ok_or_else(|| error("missing constant operand")),
        crate::ir::Operand::Value(id) => values
            .get(&id)
            .copied()
            .ok_or_else(|| error("missing SSA operand mapping")),
    }
}

#[cfg(feature = "llvm")]
fn reverse_postorder(
    ir: &Ir,
    entry: crate::ir::BlockId,
) -> Result<Vec<crate::ir::BlockId>, BackendError> {
    use crate::ir::Terminator;
    let mut visited = std::collections::HashSet::new();
    let mut pending = vec![(entry, false)];
    let mut order = Vec::new();
    while let Some((id, finish)) = pending.pop() {
        if finish {
            order.push(id);
            continue;
        }
        if !visited.insert(id) {
            continue;
        }
        pending.push((id, true));
        match &ir.blocks[id.0].terminator {
            Terminator::Jump(edge) => pending.push((edge.target, false)),
            Terminator::Branch {
                then_edge,
                else_edge,
                ..
            } => {
                pending.push((else_edge.target, false));
                pending.push((then_edge.target, false));
            }
            Terminator::Return { .. } | Terminator::Trap(_) | Terminator::Unreachable => (),
            _ => return Err(error("unsupported CFG terminator")),
        }
    }
    order.reverse();
    Ok(order)
}
