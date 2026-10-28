//! Target-derived enum unions and immutable value materialization.
use super::BackendError;
use crate::ir::*;
use inkwell::{
    basic_block::BasicBlock,
    builder::Builder,
    context::Context,
    targets::TargetData,
    types::{BasicType, BasicTypeEnum, IntType, StructType},
    values::{BasicValueEnum, PointerValue},
};
use std::collections::{HashMap, HashSet};

fn error(message: impl ToString) -> BackendError {
    BackendError::Invalid {
        message: message.to_string(),
    }
}

pub(super) struct EnumLayout<'ctx> {
    pub(super) tag: IntType<'ctx>,
    pub(super) variants: Vec<StructType<'ctx>>,
}
pub(super) type Layouts<'ctx> = HashMap<TypeId, EnumLayout<'ctx>>;

pub(super) fn complete_types<'ctx>(
    context: &'ctx Context,
    ir: &Ir,
    types: &[BasicTypeEnum<'ctx>],
    data: &TargetData,
) -> Result<Layouts<'ctx>, BackendError> {
    if ir.types.iter().any(|t| matches!(t, IrType::Enum { .. }))
        && data
            .get_data_layout()
            .as_str()
            .to_string_lossy()
            .split('-')
            .any(|s| s.starts_with("ni:"))
    {
        return Err(error("enum union storage requires integral pointers"));
    }
    let mut layouts = HashMap::new();
    let mut done: HashSet<_> = ir
        .types
        .iter()
        .enumerate()
        .filter(|(_, t)| {
            !matches!(
                t,
                IrType::Struct { .. }
                    | IrType::Enum { .. }
                    | IrType::Array { .. }
                    | IrType::Slice { .. }
                    | IrType::Tuple(_)
            )
        })
        .map(|(i, _)| TypeId(i))
        .collect();
    while done.len() < types.len() {
        let before = done.len();
        for (index, ty) in ir.types.iter().enumerate() {
            let id = TypeId(index);
            if done.contains(&id) {
                continue;
            }
            let dependencies: Vec<_> = match ty {
                IrType::Array { element, .. } => vec![element],
                IrType::Struct { fields, .. } | IrType::Tuple(fields) => fields.iter().collect(),
                IrType::Slice { .. } => vec![],
                IrType::Enum { variants, .. } => variants.iter().flat_map(|v| &v.fields).collect(),
                _ => unreachable!(),
            };
            if !dependencies.iter().all(|t| done.contains(t)) {
                continue;
            }
            let fields = match ty {
                IrType::Array { element, len } => vec![
                    types[element.0]
                        .array_type(
                            u32::try_from(*len)
                                .map_err(|_| error("array length exceeds LLVM representation"))?,
                        )
                        .into(),
                ],
                IrType::Struct { fields, .. } | IrType::Tuple(fields) => {
                    fields.iter().map(|t| types[t.0]).collect()
                }
                IrType::Slice { .. } => vec![
                    context.ptr_type(Default::default()).into(),
                    context
                        .custom_width_int_type(data.get_pointer_byte_size(None) * 8)
                        .into(),
                ],
                IrType::Enum { variants, .. } => {
                    let largest = variants.iter().map(|v| v.discriminant).max().unwrap_or(0);
                    let bits = [8, 16, 32, 64, 128]
                        .into_iter()
                        .find(|b| *b == 128 || largest < (1u128 << b))
                        .expect("128-bit discriminant");
                    let tag = context.custom_width_int_type(bits);
                    let payloads: Vec<_> = variants
                        .iter()
                        .map(|v| {
                            context.struct_type(
                                &v.fields.iter().map(|t| types[t.0]).collect::<Vec<_>>(),
                                false,
                            )
                        })
                        .collect();
                    let mut fields = vec![tag.into()];
                    if variants.iter().any(|v| !v.fields.is_empty()) {
                        let size = payloads
                            .iter()
                            .map(|p| data.get_abi_size(p))
                            .max()
                            .unwrap_or(0);
                        let anchor = payloads
                            .iter()
                            .max_by_key(|p| data.get_abi_alignment(*p))
                            .copied()
                            .expect("payload variant");
                        let size = u32::try_from(size).map_err(|_| {
                            error("enum payload exceeds supported LLVM array length")
                        })?;
                        // The zero-length anchor supplies target alignment without
                        // consuming payload bytes. LLVM adds trailing padding.
                        let carrier = context.struct_type(
                            &[
                                anchor.array_type(0).into(),
                                context.i8_type().array_type(size).into(),
                            ],
                            false,
                        );
                        fields.push(carrier.into());
                    }
                    layouts.insert(
                        id,
                        EnumLayout {
                            tag,
                            variants: payloads,
                        },
                    );
                    fields
                }
                _ => unreachable!(),
            };
            types[index].into_struct_type().set_body(&fields, false);
            if !types[index].is_sized() {
                return Err(error("unsized native aggregate"));
            }
            done.insert(id);
        }
        if done.len() == before {
            return Err(error("recursive by-value native aggregate"));
        }
    }
    Ok(layouts)
}

pub(super) struct EnumEmitter<'a, 'ctx> {
    pub context: &'ctx Context,
    pub builder: &'a Builder<'ctx>,
    pub entry: BasicBlock<'ctx>,
    pub ir: &'a Ir,
    pub types: &'a [BasicTypeEnum<'ctx>],
    pub layouts: &'a Layouts<'ctx>,
}
impl<'ctx> EnumEmitter<'_, 'ctx> {
    fn scratch(&self, ty: BasicTypeEnum<'ctx>) -> Result<PointerValue<'ctx>, BackendError> {
        let allocation = self.context.create_builder();
        if let Some(first) = self.entry.get_first_instruction() {
            allocation.position_before(&first);
        } else {
            allocation.position_at_end(self.entry);
        }
        allocation.build_alloca(ty, "enum.scratch").map_err(error)
    }
    /// Arrays use a private wrapper around one LLVM array; the Fib ABI stays internal.
    pub fn aggregate(
        &self,
        ty: TypeId,
        fields: &[BasicValueEnum<'ctx>],
    ) -> Result<BasicValueEnum<'ctx>, BackendError> {
        let outer = self.types[ty.0].into_struct_type();
        if matches!(self.ir.types[ty.0], IrType::Array { .. }) {
            let mut array = outer
                .get_field_type_at_index(0)
                .ok_or_else(|| error("missing array storage"))?
                .into_array_type()
                .const_zero();
            for (index, field) in fields.iter().enumerate() {
                array = self
                    .builder
                    .build_insert_value(array, *field, index as u32, "array.element")
                    .map_err(error)?
                    .into_array_value();
            }
            return Ok(self
                .builder
                .build_insert_value(outer.const_zero(), array, 0, "array.value")
                .map_err(error)?
                .into_struct_value()
                .into());
        }
        let mut value = outer.const_zero();
        for (index, field) in fields.iter().enumerate() {
            value = self
                .builder
                .build_insert_value(value, *field, index as u32, "aggregate.field")
                .map_err(error)?
                .into_struct_value();
        }
        Ok(value.into())
    }
    pub fn construct(
        &self,
        ty: TypeId,
        variant: usize,
        fields: &[BasicValueEnum<'ctx>],
    ) -> Result<BasicValueEnum<'ctx>, BackendError> {
        let layout = &self.layouts[&ty];
        let IrType::Enum { variants, .. } = &self.ir.types[ty.0] else {
            unreachable!()
        };
        let aggregate = self.types[ty.0].into_struct_type();
        let tag = variants[variant].discriminant;
        let tag = layout
            .tag
            .const_int_arbitrary_precision(&[tag as u64, (tag >> 64) as u64]);
        let value = self
            .builder
            .build_insert_value(aggregate.const_zero(), tag, 0, "enum.tag")
            .map_err(error)?
            .into_struct_value();
        if fields.is_empty() {
            return Ok(value.into());
        }
        let scratch = self.scratch(aggregate.into())?;
        self.builder.build_store(scratch, value).map_err(error)?;
        let payload = self
            .builder
            .build_struct_gep(aggregate, scratch, 1, "enum.payload")
            .map_err(error)?;
        let mut value = layout.variants[variant].const_zero();
        for (index, field) in fields.iter().enumerate() {
            value = self
                .builder
                .build_insert_value(value, *field, index as u32, "enum.field")
                .map_err(error)?
                .into_struct_value();
        }
        self.builder.build_store(payload, value).map_err(error)?;
        self.builder
            .build_load(aggregate, scratch, "enum.value")
            .map_err(error)
    }
    pub fn field(
        &self,
        ty: TypeId,
        value: BasicValueEnum<'ctx>,
        variant: usize,
        field: usize,
    ) -> Result<BasicValueEnum<'ctx>, BackendError> {
        let aggregate = self.types[ty.0].into_struct_type();
        let scratch = self.scratch(aggregate.into())?;
        self.builder.build_store(scratch, value).map_err(error)?;
        let payload = self
            .builder
            .build_struct_gep(aggregate, scratch, 1, "enum.payload")
            .map_err(error)?;
        let payload_ty = self.layouts[&ty].variants[variant];
        let address = self
            .builder
            .build_struct_gep(payload_ty, payload, field as u32, "enum.field.address")
            .map_err(error)?;
        let field_ty = payload_ty
            .get_field_type_at_index(field as u32)
            .ok_or_else(|| error("missing enum field type"))?;
        self.builder
            .build_load(field_ty, address, "enum.field")
            .map_err(error)
    }
    pub fn constants(
        &self,
        native: &[BasicValueEnum<'ctx>],
        blocks: &[BlockId],
    ) -> Result<Vec<BasicValueEnum<'ctx>>, BackendError> {
        let used = constant_uses(self.ir, blocks);
        let mut values = Vec::with_capacity(native.len());
        for (index, (constant, value)) in self.ir.constants.iter().zip(native).enumerate() {
            let logical = self.types[constant.ty.0];
            if value.get_type() == logical || !used.contains(&ConstantId(index)) {
                values.push(*value);
            } else {
                // Physical constant layouts have already been checked against
                // logical layouts. Loading through opaque pointers restores the
                // canonical SSA type without losing relocation-bearing fields.
                let scratch = self.scratch(value.get_type())?;
                self.builder.build_store(scratch, *value).map_err(error)?;
                values.push(
                    self.builder
                        .build_load(logical, scratch, "constant.value")
                        .map_err(error)?,
                );
            }
        }
        Ok(values)
    }
}

/// Global initializers are not function operands. In particular, a large static
/// array must not be copied onto every function's stack merely because its
/// initializer appears in the module constant arena.
fn constant_uses(ir: &Ir, blocks: &[BlockId]) -> HashSet<ConstantId> {
    let mut used = HashSet::new();
    let mut operands = Vec::new();
    for block in blocks.iter().map(|id| &ir.blocks[id.0]) {
        for instruction in &block.instructions {
            use InstructionKind as I;
            match &instruction.kind {
                I::Constant(id) => {
                    used.insert(*id);
                }
                I::Binary { lhs, rhs, .. } => operands.extend([*lhs, *rhs]),
                I::Unary { operand, .. }
                | I::Cast { operand, .. }
                | I::SliceData(operand)
                | I::SliceLength(operand)
                | I::EnumTag { value: operand }
                | I::EnumField { value: operand, .. } => operands.push(*operand),
                I::Call { args, .. } => operands.extend(args),
                I::CallIndirect { callee, args } => {
                    operands.push(*callee);
                    operands.extend(args);
                }
                I::Syscall { number, args } => {
                    operands.push(*number);
                    operands.extend(args);
                }
                I::Slice { data, length } => operands.extend([*data, *length]),
                I::ArrayData(base)
                | I::FieldAddress { base, .. }
                | I::FunctionPointerAddress { base, .. } => operands.push(base.0),
                I::Load { address } => operands.push(address.0),
                I::Store { address, value } => operands.extend([address.0, *value]),
                I::Aggregate { elements }
                | I::Enum {
                    fields: elements, ..
                } => operands.extend(elements),
                I::ExtractField { aggregate, .. } => operands.push(*aggregate),
                I::InsertField {
                    aggregate, value, ..
                } => operands.extend([*aggregate, *value]),
                I::ExtractElement { aggregate, index } => operands.extend([*aggregate, *index]),
                I::ElementAddress { base, index } => operands.extend([base.0, *index]),
                I::FunctionAddress(_) | I::StackAddress(_) | I::GlobalAddress(_) => {}
            }
        }
        match &block.terminator {
            Terminator::Jump(edge) => operands.extend(&edge.args),
            Terminator::Branch {
                condition,
                then_edge,
                else_edge,
            } => {
                operands.push(*condition);
                operands.extend(&then_edge.args);
                operands.extend(&else_edge.args);
            }
            Terminator::Switch {
                subject,
                cases,
                default,
            } => {
                operands.push(*subject);
                operands.extend(&default.args);
                for case in cases {
                    used.insert(case.value);
                    operands.extend(&case.edge.args);
                }
            }
            Terminator::Return { values } => operands.extend(values),
            Terminator::Trap(_) | Terminator::Unreachable => {}
        }
    }
    used.extend(operands.into_iter().filter_map(|operand| match operand {
        Operand::Constant(id) => Some(id),
        Operand::Value(_) => None,
    }));
    used
}
