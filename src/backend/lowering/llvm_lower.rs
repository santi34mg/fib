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
    use inkwell::{context::Context, types::BasicTypeEnum, values::BasicValue};
    let context = Context::create();
    let module = context.create_module("fib");
    let machine = machine(target)?;
    module.set_triple(&machine.get_triple());
    module.set_data_layout(&machine.get_target_data().get_data_layout());
    let types: Vec<_> = ir
        .types
        .iter()
        .enumerate()
        .map(|(index, ty)| match ty {
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
            IrType::Integer {
                width: IntegerWidth::Pointer,
                ..
            } => Ok(context
                .custom_width_int_type(machine.get_target_data().get_pointer_byte_size(None) * 8)
                .into()),
            IrType::Array { .. } => Ok(context
                .opaque_struct_type(&format!("fib.array.{index}"))
                .into()),
            IrType::Pointer { .. } => Ok(context.ptr_type(Default::default()).into()),
            IrType::Struct { .. } => Ok(context
                .opaque_struct_type(&format!("fib.struct.{index}"))
                .into()),
            IrType::Enum { .. } => Ok(context
                .opaque_struct_type(&format!("fib.enum.{index}"))
                .into()),
            _ => Err(error("unsupported LLVM type")),
        })
        .collect::<Result<_, _>>()?;
    let enum_layouts =
        super::llvm_enum::complete_types(&context, ir, &types, &machine.get_target_data())?;
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
    let builder = context.create_builder();
    for (fi, f) in ir.functions.iter().enumerate() {
        let FunctionBody::Definition {
            entry,
            blocks,
            stack_slots,
        } = &f.body
        else {
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
        builder.position_at_end(llvm_blocks[entry]);
        let enum_emitter = super::llvm_enum::EnumEmitter {
            context: &context,
            builder: &builder,
            entry: llvm_blocks[entry],
            ir,
            types: &types,
            layouts: &enum_layouts,
        };
        let constants = enum_emitter.constants()?;
        let mut slots = std::collections::HashMap::new();
        for &slot in stack_slots {
            let allocation = builder
                .build_alloca(types[ir.stack_slots[slot.0].ty.0], "slot")
                .map_err(error)?;
            slots.insert(slot, allocation);
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
                        if matches!(ir.types[ty.0], IrType::Pointer { .. }) {
                            let predicate = match op {
                                BinaryOp::Eq => inkwell::IntPredicate::EQ,
                                BinaryOp::Ne => inkwell::IntPredicate::NE,
                                _ => return Err(error("unsupported pointer comparison")),
                            };
                            let value = builder
                                .build_int_compare(
                                    predicate,
                                    operand(*lhs, &constants, &values)?.into_pointer_value(),
                                    operand(*rhs, &constants, &values)?.into_pointer_value(),
                                    "pointer.cmp",
                                )
                                .map_err(error)?;
                            values.insert(inst.results[0], value.into());
                            continue;
                        }
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
                    InstructionKind::StackAddress(slot) => {
                        values.insert(
                            inst.results[0],
                            (*slots.get(slot).ok_or_else(|| error("missing stack slot"))?).into(),
                        );
                    }
                    InstructionKind::Load { address } => {
                        let value = builder
                            .build_load(
                                types[ir.values[inst.results[0].0].ty.0],
                                operand(address.0, &constants, &values)?.into_pointer_value(),
                                "load",
                            )
                            .map_err(error)?;
                        value
                            .as_instruction_value()
                            .ok_or_else(|| error("missing load instruction"))?
                            .set_alignment(1)
                            .map_err(error)?;
                        values.insert(inst.results[0], value);
                    }
                    InstructionKind::Store { address, value } => {
                        builder
                            .build_store(
                                operand(address.0, &constants, &values)?.into_pointer_value(),
                                operand(*value, &constants, &values)?,
                            )
                            .map_err(error)?
                            .set_alignment(1)
                            .map_err(error)?;
                    }
                    InstructionKind::FieldAddress { base, field } => {
                        let pointer = operand(base.0, &constants, &values)?.into_pointer_value();
                        let Some(IrType::Pointer { pointee }) =
                            ir.operand_type(base.0).and_then(|ty| ir.get_type(ty))
                        else {
                            return Err(error("missing projection pointee"));
                        };
                        let indices = [
                            context.i32_type().const_zero(),
                            context.i32_type().const_int(*field as u64, false),
                        ];
                        // Shared verification proves that the pointee is a struct
                        // and the field index belongs to it. Do not promise inbounds.
                        let value = unsafe {
                            builder.build_gep(types[pointee.0], pointer, &indices, "field.address")
                        }
                        .map_err(error)?;
                        values.insert(inst.results[0], value.into());
                    }
                    InstructionKind::Aggregate { elements } => {
                        let elements = elements
                            .iter()
                            .map(|e| operand(*e, &constants, &values))
                            .collect::<Result<Vec<_>, _>>()?;
                        let value =
                            enum_emitter.aggregate(ir.values[inst.results[0].0].ty, &elements)?;
                        values.insert(inst.results[0], value);
                    }
                    InstructionKind::ExtractField { aggregate, field } => {
                        let value = builder
                            .build_extract_value(
                                operand(*aggregate, &constants, &values)?.into_struct_value(),
                                *field as u32,
                                "field.extract",
                            )
                            .map_err(error)?;
                        values.insert(inst.results[0], value);
                    }
                    InstructionKind::InsertField {
                        aggregate,
                        field,
                        value,
                    } => {
                        let value = builder
                            .build_insert_value(
                                operand(*aggregate, &constants, &values)?.into_struct_value(),
                                operand(*value, &constants, &values)?,
                                *field as u32,
                                "field.insert",
                            )
                            .map_err(error)?
                            .into_struct_value();
                        values.insert(inst.results[0], value.into());
                    }
                    InstructionKind::Enum { variant, fields } => {
                        let ty = ir.values[inst.results[0].0].ty;
                        let fields = fields
                            .iter()
                            .map(|f| operand(*f, &constants, &values))
                            .collect::<Result<Vec<_>, _>>()?;
                        values.insert(
                            inst.results[0],
                            enum_emitter.construct(ty, *variant, &fields)?,
                        );
                    }
                    InstructionKind::EnumTag { value } => {
                        let tag = builder
                            .build_extract_value(
                                operand(*value, &constants, &values)?.into_struct_value(),
                                0,
                                "enum.tag",
                            )
                            .map_err(error)?
                            .into_int_value();
                        let dest = types[ir.values[inst.results[0].0].ty.0].into_int_type();
                        values.insert(
                            inst.results[0],
                            builder
                                .build_int_cast_sign_flag(tag, dest, false, "enum.tag.cast")
                                .map_err(error)?
                                .into(),
                        );
                    }
                    InstructionKind::EnumField {
                        value,
                        variant,
                        field,
                    } => {
                        let ty = ir
                            .operand_type(*value)
                            .ok_or_else(|| error("missing enum type"))?;
                        values.insert(
                            inst.results[0],
                            enum_emitter.field(
                                ty,
                                operand(*value, &constants, &values)?,
                                *variant,
                                *field,
                            )?,
                        );
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
                Terminator::Switch {
                    subject,
                    cases,
                    default,
                } => {
                    // Give every successor occurrence its own predecessor. This
                    // also preserves parallel cases/default carrying different PHI inputs.
                    let mut successors = Vec::new();
                    for edge in cases
                        .iter()
                        .map(|c| &c.edge)
                        .chain(std::iter::once(default))
                    {
                        let trampoline = context.append_basic_block(functions[fi], "switch.edge");
                        successors.push((edge, trampoline));
                    }
                    let case_values = cases
                        .iter()
                        .zip(&successors)
                        .map(|(c, (_, block))| Ok((constants[c.value.0].into_int_value(), *block)))
                        .collect::<Result<Vec<_>, BackendError>>()?;
                    builder
                        .build_switch(
                            operand(*subject, &constants, &values)?.into_int_value(),
                            successors.last().expect("default successor").1,
                            &case_values,
                        )
                        .map_err(error)?;
                    for (edge, trampoline) in successors {
                        builder.position_at_end(trampoline);
                        builder
                            .build_unconditional_branch(llvm_blocks[&edge.target])
                            .map_err(error)?;
                        incoming.push((edge, trampoline));
                    }
                }
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
            Terminator::Switch { cases, default, .. } => {
                pending.push((default.target, false));
                pending.extend(cases.iter().rev().map(|c| (c.edge.target, false)));
            }
        }
    }
    order.reverse();
    Ok(order)
}
