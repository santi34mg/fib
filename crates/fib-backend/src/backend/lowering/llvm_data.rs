//! Relocation-preserving native constants. Physical enum payloads retain their
//! typed fields; every substituted layout is checked against the runtime layout.
use super::{BackendError, llvm_enum::Layouts};
use crate::ir::*;
use inkwell::{
    context::Context,
    module::Module,
    targets::TargetData,
    types::{BasicTypeEnum, StructType},
    values::{BasicValueEnum, FunctionValue, GlobalValue},
};

fn error(message: impl ToString) -> BackendError {
    BackendError::Invalid {
        message: message.to_string(),
    }
}

pub(super) struct NativeData<'ctx> {
    pub globals: Vec<GlobalValue<'ctx>>,
    pub constants: Vec<BasicValueEnum<'ctx>>,
}

struct Emitter<'a, 'ctx> {
    context: &'ctx Context,
    module: &'a Module<'ctx>,
    ir: &'a Ir,
    types: &'a [BasicTypeEnum<'ctx>],
    layouts: &'a Layouts<'ctx>,
    target: &'a TargetData,
}

impl<'ctx> Emitter<'_, 'ctx> {
    fn compatible(
        &self,
        physical: BasicTypeEnum<'ctx>,
        logical: BasicTypeEnum<'ctx>,
    ) -> Result<(), BackendError> {
        if self.target.get_abi_size(&physical) != self.target.get_abi_size(&logical)
            || self.target.get_abi_alignment(&physical) != self.target.get_abi_alignment(&logical)
        {
            return Err(error("constant storage layout differs from runtime layout"));
        }
        Ok(())
    }

    fn structure(
        &self,
        fields: &[BasicTypeEnum<'ctx>],
        logical: StructType<'ctx>,
    ) -> Result<StructType<'ctx>, BackendError> {
        if fields == logical.get_field_types() {
            return Ok(logical);
        }
        let physical = self.context.struct_type(fields, false);
        self.compatible(physical.into(), logical.into())?;
        if fields.len() != logical.count_fields() as usize {
            return Err(error("constant field count mismatch"));
        }
        for index in 0..fields.len() as u32 {
            if self.target.offset_of_element(&physical, index)
                != self.target.offset_of_element(&logical, index)
            {
                return Err(error("constant field offset differs from runtime layout"));
            }
        }
        Ok(physical)
    }

    fn storage_types(&self) -> Result<Vec<BasicTypeEnum<'ctx>>, BackendError> {
        let mut result = vec![None; self.ir.constants.len()];
        while result.iter().any(Option::is_none) {
            let before = result.iter().filter(|ty| ty.is_some()).count();
            for (index, constant) in self.ir.constants.iter().enumerate() {
                if result[index].is_some() {
                    continue;
                }
                let logical = self.types[constant.ty.0];
                let fields = match &constant.kind {
                    ConstantKind::Aggregate(fields) | ConstantKind::Enum { fields, .. } => {
                        let Some(fields) = fields
                            .iter()
                            .map(|id| result[id.0])
                            .collect::<Option<Vec<_>>>()
                        else {
                            continue;
                        };
                        fields
                    }
                    _ => vec![],
                };
                result[index] = Some(match (&constant.kind, &self.ir.types[constant.ty.0]) {
                    (ConstantKind::Aggregate(_), IrType::Array { element, len }) if *len > 0 => {
                        let inner = self.context.struct_type(&fields, false);
                        let logical_element = self.types[element.0];
                        for (index, field) in fields.iter().enumerate() {
                            self.compatible(*field, logical_element)?;
                            if self.target.offset_of_element(&inner, index as u32)
                                != Some(index as u64 * self.target.get_abi_size(&logical_element))
                            {
                                return Err(error(
                                    "constant array stride differs from runtime layout",
                                ));
                            }
                        }
                        let outer = self.context.struct_type(&[inner.into()], false);
                        self.compatible(outer.into(), logical)?;
                        outer.into()
                    }
                    (ConstantKind::Aggregate(_), IrType::Struct { .. } | IrType::Tuple(_)) => {
                        self.structure(&fields, logical.into_struct_type())?.into()
                    }
                    (ConstantKind::Enum { variant, .. }, _) => {
                        let layout = &self.layouts[&constant.ty];
                        let outer = logical.into_struct_type();
                        if outer.count_fields() == 1 {
                            logical
                        } else {
                            let carrier =
                                outer.get_field_type_at_index(1).unwrap().into_struct_type();
                            let anchor = carrier.get_field_type_at_index(0).unwrap();
                            let capacity = carrier
                                .get_field_type_at_index(1)
                                .unwrap()
                                .into_array_type()
                                .len() as u64;
                            let payload = self.structure(&fields, layout.variants[*variant])?;
                            let size = self.target.get_abi_size(&payload);
                            let padding = capacity
                                .checked_sub(size)
                                .ok_or_else(|| error("constant enum payload exceeds carrier"))?;
                            let payload_carrier = self.context.struct_type(
                                &[
                                    anchor,
                                    payload.into(),
                                    self.context
                                        .i8_type()
                                        .array_type(u32::try_from(padding).map_err(error)?)
                                        .into(),
                                ],
                                false,
                            );
                            self.compatible(payload_carrier.into(), carrier.into())?;
                            if self.target.offset_of_element(&payload_carrier, 1) != Some(0) {
                                return Err(error("constant enum payload offset mismatch"));
                            }
                            self.structure(&[layout.tag.into(), payload_carrier.into()], outer)?
                                .into()
                        }
                    }
                    _ => logical,
                });
            }
            if result.iter().filter(|ty| ty.is_some()).count() == before {
                return Err(error("unresolved constant storage dependencies"));
            }
        }
        Ok(result.into_iter().map(Option::unwrap).collect())
    }

    fn byte_global(&self, index: usize, bytes: &[u8], terminated: bool) -> GlobalValue<'ctx> {
        let value = self.context.const_string(bytes, terminated);
        let global = self
            .module
            .add_global(value.get_type(), None, &format!("fib.bytes.{index}"));
        global.set_linkage(inkwell::module::Linkage::Private);
        global.set_initializer(&value);
        global.set_constant(true);
        global
    }

    fn constants(
        &self,
        storage: &[BasicTypeEnum<'ctx>],
        globals: &[GlobalValue<'ctx>],
        functions: &[FunctionValue<'ctx>],
    ) -> Result<Vec<BasicValueEnum<'ctx>>, BackendError> {
        let mut result: Vec<Option<BasicValueEnum<'ctx>>> = vec![None; self.ir.constants.len()];
        while result.iter().any(Option::is_none) {
            let before = result.iter().filter(|value| value.is_some()).count();
            for (index, constant) in self.ir.constants.iter().enumerate() {
                if result[index].is_some() {
                    continue;
                }
                let ty = storage[index];
                let value = match &constant.kind {
                    ConstantKind::Bool(value) => ty
                        .into_int_type()
                        .const_int(u64::from(*value), false)
                        .into(),
                    ConstantKind::Integer { bits } => {
                        let integer = ty.into_int_type();
                        if integer.get_bit_width() < 128
                            && *bits >= (1u128 << integer.get_bit_width())
                        {
                            return Err(error("integer constant exceeds target width"));
                        }
                        integer
                            .const_int_arbitrary_precision(&[*bits as u64, (*bits >> 64) as u64])
                            .into()
                    }
                    ConstantKind::Float { bits } => {
                        super::llvm_scalar::float_constant(ty.into_float_type(), *bits).into()
                    }
                    ConstantKind::Null => ty.into_pointer_type().const_null().into(),
                    ConstantKind::GlobalAddress(id) => globals[id.0].as_pointer_value().into(),
                    ConstantKind::FunctionAddress(id) => {
                        functions[id.0].as_global_value().as_pointer_value().into()
                    }
                    ConstantKind::PointerCast(base) => {
                        let Some(value) = result[base.0] else {
                            continue;
                        };
                        value
                    }
                    ConstantKind::CString(bytes) => self
                        .byte_global(index, bytes, true)
                        .as_pointer_value()
                        .into(),
                    ConstantKind::Bytes(bytes) => match &self.ir.types[constant.ty.0] {
                        IrType::Array { .. } => ty
                            .into_struct_type()
                            .const_named_struct(&[self.context.const_string(bytes, false).into()])
                            .into(),
                        IrType::Slice { .. } => {
                            let pointer = self.byte_global(index, bytes, false).as_pointer_value();
                            let structure = ty.into_struct_type();
                            let length = structure
                                .get_field_type_at_index(1)
                                .unwrap()
                                .into_int_type()
                                .const_int(bytes.len() as u64, false);
                            structure
                                .const_named_struct(&[pointer.into(), length.into()])
                                .into()
                        }
                        _ => return Err(error("interpreter bytes cannot be emitted natively")),
                    },
                    ConstantKind::Aggregate(fields) | ConstantKind::Enum { fields, .. } => {
                        let Some(fields) = fields
                            .iter()
                            .map(|id| result[id.0])
                            .collect::<Option<Vec<_>>>()
                        else {
                            continue;
                        };
                        match &constant.kind {
                            ConstantKind::Enum { variant, .. } => {
                                let layout = &self.layouts[&constant.ty];
                                let IrType::Enum { variants, .. } = &self.ir.types[constant.ty.0]
                                else {
                                    unreachable!()
                                };
                                let tag = layout.tag.const_int_arbitrary_precision(&[
                                    variants[*variant].discriminant as u64,
                                    (variants[*variant].discriminant >> 64) as u64,
                                ]);
                                let outer = ty.into_struct_type();
                                let mut elements = vec![tag.into()];
                                if outer.count_fields() > 1 {
                                    let carrier = outer
                                        .get_field_type_at_index(1)
                                        .unwrap()
                                        .into_struct_type();
                                    let payload = carrier
                                        .get_field_type_at_index(1)
                                        .unwrap()
                                        .into_struct_type()
                                        .const_named_struct(&fields);
                                    let anchor =
                                        carrier.get_field_type_at_index(0).unwrap().const_zero();
                                    let padding =
                                        carrier.get_field_type_at_index(2).unwrap().const_zero();
                                    elements.push(
                                        carrier
                                            .const_named_struct(&[anchor, payload.into(), padding])
                                            .into(),
                                    );
                                }
                                outer.const_named_struct(&elements).into()
                            }
                            _ if matches!(
                                self.ir.types[constant.ty.0],
                                IrType::Array { len: 0, .. }
                            ) =>
                            {
                                ty.const_zero()
                            }
                            _ if matches!(self.ir.types[constant.ty.0], IrType::Array { .. }) => {
                                let outer = ty.into_struct_type();
                                let inner = outer
                                    .get_field_type_at_index(0)
                                    .unwrap()
                                    .into_struct_type()
                                    .const_named_struct(&fields);
                                outer.const_named_struct(&[inner.into()]).into()
                            }
                            _ => ty.into_struct_type().const_named_struct(&fields).into(),
                        }
                    }
                    ConstantKind::FieldAddress { base, .. }
                    | ConstantKind::ElementAddress { base, .. } => {
                        let Some(base_value) = result[base.0] else {
                            continue;
                        };
                        let IrType::Pointer { pointee } =
                            self.ir.types[self.ir.constants[base.0].ty.0]
                        else {
                            unreachable!()
                        };
                        let offset = match &constant.kind {
                            ConstantKind::ElementAddress { index, .. } => *index,
                            ConstantKind::FieldAddress { field, .. } => *field as u64,
                            _ => unreachable!(),
                        };
                        let width = self.target.get_pointer_byte_size(None) * 8;
                        if width < 64 && offset >= (1u64 << width) {
                            return Err(error("constant address offset exceeds target width"));
                        }
                        let indices = if matches!(constant.kind, ConstantKind::FieldAddress { .. })
                        {
                            vec![
                                self.context.i32_type().const_zero(),
                                self.context.i32_type().const_int(
                                    u64::from(u32::try_from(offset).map_err(error)?),
                                    false,
                                ),
                            ]
                        } else {
                            vec![
                                self.context
                                    .custom_width_int_type(width)
                                    .const_int(offset, false),
                            ]
                        };
                        unsafe {
                            base_value
                                .into_pointer_value()
                                .const_gep(self.types[pointee.0], &indices)
                        }
                        .into()
                    }
                    ConstantKind::ArrayData(base) => {
                        let Some(base_value) = result[base.0] else {
                            continue;
                        };
                        let IrType::Pointer { pointee } =
                            self.ir.types[self.ir.constants[base.0].ty.0]
                        else {
                            unreachable!()
                        };
                        let zero = self.context.i32_type().const_zero();
                        unsafe {
                            base_value
                                .into_pointer_value()
                                .const_gep(self.types[pointee.0], &[zero, zero, zero])
                        }
                        .into()
                    }
                    ConstantKind::Slice { data, length } => {
                        let Some(data) = result[data.0] else {
                            continue;
                        };
                        let structure = ty.into_struct_type();
                        let length_type = structure
                            .get_field_type_at_index(1)
                            .unwrap()
                            .into_int_type();
                        if length_type.get_bit_width() < 64
                            && *length >= (1u64 << length_type.get_bit_width())
                        {
                            return Err(error("slice length exceeds target width"));
                        }
                        structure
                            .const_named_struct(&[
                                data,
                                length_type.const_int(*length, false).into(),
                            ])
                            .into()
                    }
                };
                result[index] = Some(value);
            }
            if result.iter().filter(|value| value.is_some()).count() == before {
                return Err(error("unresolved constant data dependencies"));
            }
        }
        Ok(result.into_iter().map(Option::unwrap).collect())
    }
}

pub(super) fn emit<'ctx>(
    context: &'ctx Context,
    module: &Module<'ctx>,
    ir: &Ir,
    types: &[BasicTypeEnum<'ctx>],
    layouts: &Layouts<'ctx>,
    target: &TargetData,
    functions: &[FunctionValue<'ctx>],
) -> Result<NativeData<'ctx>, BackendError> {
    let emitter = Emitter {
        context,
        module,
        ir,
        types,
        layouts,
        target,
    };
    let storage = emitter.storage_types()?;
    let globals = ir
        .globals
        .iter()
        .map(|global| {
            let ty = match global.initializer {
                GlobalInitializer::Constant(id) => storage[id.0],
                GlobalInitializer::Declaration => types[global.ty.0],
            };
            let value = module.add_global(ty, None, &global.name);
            value.set_linkage(match global.linkage {
                Linkage::Internal => inkwell::module::Linkage::Internal,
                Linkage::External => inkwell::module::Linkage::External,
            });
            value.set_constant(!global.mutable);
            value
        })
        .collect::<Vec<_>>();
    let constants = emitter.constants(&storage, &globals, functions)?;
    for (global, value) in ir.globals.iter().zip(&globals) {
        if let GlobalInitializer::Constant(id) = global.initializer {
            value.set_initializer(&constants[id.0]);
        }
    }
    Ok(NativeData { globals, constants })
}
