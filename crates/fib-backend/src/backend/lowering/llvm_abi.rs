//! Shared physical ABI adaptation for definitions, direct calls and callbacks.
use super::{
    BackendError,
    abi::{AbiPlan, AbiTarget, FunctionAbi, PhysicalConvention},
    c_abi::{Boundary, Piece, PieceKind},
};
use crate::ir::*;
use inkwell::{
    attributes::{Attribute, AttributeLoc},
    builder::Builder,
    context::Context,
    module::{Linkage as LlvmLinkage, Module},
    types::{AnyType, BasicMetadataTypeEnum, BasicType, BasicTypeEnum, FunctionType},
    values::{
        BasicMetadataValueEnum, BasicValue, BasicValueEnum, CallSiteValue, FunctionValue,
        PointerValue,
    },
};

pub(super) struct LlvmAbi<'a, 'ctx> {
    pub context: &'ctx Context,
    pub plan: &'a AbiPlan,
    pub types: &'a [BasicTypeEnum<'ctx>],
    pub ir: &'a Ir,
    pub target: &'a AbiTarget,
}
impl<'ctx> LlvmAbi<'_, 'ctx> {
    fn piece_type(&self, p: &Piece) -> BasicTypeEnum<'ctx> {
        match p.kind {
            PieceKind::Integer(w) => self.context.custom_width_int_type(w.into()).into(),
            PieceKind::Float32 => self.context.f32_type().into(),
            PieceKind::Float64 => self.context.f64_type().into(),
        }
    }
    fn result_type(&self, abi: &FunctionAbi) -> Option<BasicTypeEnum<'ctx>> {
        let logical = abi.result?;
        match abi.c_abi.as_ref().and_then(|c| c.result.as_ref()) {
            Some(Boundary::Indirect { .. }) => None,
            Some(Boundary::Coerce(pieces)) => match pieces.as_slice() {
                [p] => Some(self.piece_type(p)),
                _ => Some(
                    self.context
                        .struct_type(
                            &pieces
                                .iter()
                                .map(|p| self.piece_type(p))
                                .collect::<Vec<_>>(),
                            false,
                        )
                        .into(),
                ),
            },
            _ => Some(self.types[logical.0]),
        }
    }
    fn function_type(&self, abi: &FunctionAbi) -> FunctionType<'ctx> {
        let mut params = Vec::new();
        if let Some(c) = &abi.c_abi {
            if matches!(c.result, Some(Boundary::Indirect { .. })) {
                params.push(self.context.ptr_type(Default::default()).into());
            }
            for (ty, boundary) in abi.parameters.iter().zip(&c.arguments) {
                match boundary {
                    Boundary::Direct => params.push(self.types[ty.0].into()),
                    Boundary::Indirect { .. } => {
                        params.push(self.context.ptr_type(Default::default()).into())
                    }
                    Boundary::Coerce(pieces) => params.extend(
                        pieces
                            .iter()
                            .map(|p| BasicMetadataTypeEnum::from(self.piece_type(p))),
                    ),
                }
            }
        } else {
            params.extend(
                abi.parameters
                    .iter()
                    .map(|ty| BasicMetadataTypeEnum::from(self.types[ty.0])),
            );
        }
        match self.result_type(abi) {
            Some(t) => t.fn_type(&params, false),
            None => self.context.void_type().fn_type(&params, false),
        }
    }
    fn attributes(&self, abi: &FunctionAbi) -> Vec<(AttributeLoc, Attribute)> {
        let mut attrs = Vec::new();
        let Some(c) = &abi.c_abi else {
            return attrs;
        };
        let mut index = 0;
        if let Some(Boundary::Indirect { alignment }) = &c.result {
            let ty = self.types[abi.result.unwrap().0];
            attrs.push((
                AttributeLoc::Param(0),
                self.context.create_type_attribute(
                    Attribute::get_named_enum_kind_id("sret"),
                    ty.as_any_type_enum(),
                ),
            ));
            attrs.push((
                AttributeLoc::Param(0),
                self.context.create_enum_attribute(
                    Attribute::get_named_enum_kind_id("align"),
                    (*alignment).into(),
                ),
            ));
            index += 1;
        }
        for (&ty, boundary) in abi.parameters.iter().zip(&c.arguments) {
            match boundary {
                Boundary::Indirect { alignment } => {
                    attrs.push((
                        AttributeLoc::Param(index),
                        self.context.create_type_attribute(
                            Attribute::get_named_enum_kind_id("byval"),
                            self.types[ty.0].as_any_type_enum(),
                        ),
                    ));
                    attrs.push((
                        AttributeLoc::Param(index),
                        self.context.create_enum_attribute(
                            Attribute::get_named_enum_kind_id("align"),
                            u64::from((*alignment).max(8)),
                        ),
                    ));
                    index += 1;
                }
                Boundary::Coerce(pieces) => index += pieces.len() as u32,
                Boundary::Direct => {
                    if let Some(a) = self.extension(ty) {
                        attrs.push((AttributeLoc::Param(index), a));
                    }
                    index += 1;
                }
            }
        }
        if matches!(c.result, Some(Boundary::Direct))
            && let Some(a) = self.extension(abi.result.unwrap())
        {
            attrs.push((AttributeLoc::Return, a));
        }
        attrs
    }
    fn extension(&self, ty: TypeId) -> Option<Attribute> {
        let name = match self.ir.types[ty.0] {
            IrType::Bool => "zeroext",
            IrType::Integer {
                width: IntegerWidth::Bits(8 | 16),
                signed,
            } => {
                if signed {
                    "signext"
                } else {
                    "zeroext"
                }
            }
            _ => return None,
        };
        Some(
            self.context
                .create_enum_attribute(Attribute::get_named_enum_kind_id(name), 0),
        )
    }
    pub fn declare(
        &self,
        module: &Module<'ctx>,
        id: FunctionId,
        function: &Function,
    ) -> Result<FunctionValue<'ctx>, BackendError> {
        let abi = self.plan.function(id)?;
        let linkage = match function.linkage {
            Linkage::Internal => LlvmLinkage::Internal,
            Linkage::External => LlvmLinkage::External,
        };
        let value =
            module.add_function(&function.identifier, self.function_type(abi), Some(linkage));
        value.set_call_conventions(convention(abi.convention));
        for (location, attribute) in self.attributes(abi) {
            value.add_attribute(location, attribute);
        }
        Ok(value)
    }
    // Scratch storage is allocated once in entry, including calls in loops.
    fn scratch(
        &self,
        builder: &Builder<'ctx>,
        ty: BasicTypeEnum<'ctx>,
    ) -> Result<PointerValue<'ctx>, BackendError> {
        let entry = builder
            .get_insert_block()
            .and_then(|b| b.get_parent())
            .and_then(|f| f.get_first_basic_block())
            .ok_or_else(|| error("ABI scratch requires a function"))?;
        let allocator = self.context.create_builder();
        match entry.get_first_instruction() {
            Some(i) => allocator.position_before(&i),
            None => allocator.position_at_end(entry),
        };
        allocator.build_alloca(ty, "abi.scratch").map_err(error)
    }
    fn offset(
        &self,
        builder: &Builder<'ctx>,
        pointer: PointerValue<'ctx>,
        offset: u64,
    ) -> Result<PointerValue<'ctx>, BackendError> {
        unsafe {
            builder.build_gep(
                self.context.i8_type(),
                pointer,
                &[self.context.i64_type().const_int(offset, false)],
                "abi.part",
            )
        }
        .map_err(error)
    }
    fn pack(
        &self,
        builder: &Builder<'ctx>,
        ty: TypeId,
        pieces: &[Piece],
        values: &[BasicValueEnum<'ctx>],
    ) -> Result<BasicValueEnum<'ctx>, BackendError> {
        let pointer = self.scratch(builder, self.types[ty.0])?;
        builder
            .build_store(pointer, self.types[ty.0].const_zero())
            .map_err(error)?;
        for (piece, value) in pieces.iter().zip(values) {
            builder
                .build_store(self.offset(builder, pointer, piece.offset)?, *value)
                .map_err(error)?
                .set_alignment(1)
                .map_err(error)?;
        }
        builder
            .build_load(self.types[ty.0], pointer, "abi.logical")
            .map_err(error)
    }
    fn unpack(
        &self,
        builder: &Builder<'ctx>,
        value: BasicValueEnum<'ctx>,
        pieces: &[Piece],
    ) -> Result<Vec<BasicValueEnum<'ctx>>, BackendError> {
        let pointer = self.scratch(builder, value.get_type())?;
        builder.build_store(pointer, value).map_err(error)?;
        pieces
            .iter()
            .map(|p| {
                let v = builder
                    .build_load(
                        self.piece_type(p),
                        self.offset(builder, pointer, p.offset)?,
                        "abi.physical",
                    )
                    .map_err(error)?;
                v.as_instruction_value()
                    .unwrap()
                    .set_alignment(1)
                    .map_err(error)?;
                Ok(v)
            })
            .collect()
    }
    pub fn arguments(
        &self,
        builder: &Builder<'ctx>,
        id: FunctionId,
        function: FunctionValue<'ctx>,
    ) -> Result<Vec<BasicValueEnum<'ctx>>, BackendError> {
        let abi = self.plan.function(id)?;
        let Some(c) = &abi.c_abi else {
            return abi
                .parameter_positions
                .iter()
                .map(|p| {
                    function
                        .get_nth_param(*p as u32)
                        .ok_or_else(|| error("missing physical argument"))
                })
                .collect();
        };
        let mut index = usize::from(matches!(c.result, Some(Boundary::Indirect { .. }))) as u32;
        let mut args = Vec::new();
        for (&ty, boundary) in abi.parameters.iter().zip(&c.arguments) {
            let value = match boundary {
                Boundary::Direct => {
                    let v = function.get_nth_param(index).unwrap();
                    index += 1;
                    v
                }
                Boundary::Indirect { .. } => {
                    let p = function.get_nth_param(index).unwrap().into_pointer_value();
                    index += 1;
                    builder
                        .build_load(self.types[ty.0], p, "abi.argument")
                        .map_err(error)?
                }
                Boundary::Coerce(pieces) => {
                    let values = (0..pieces.len())
                        .map(|_| {
                            let v = function.get_nth_param(index).unwrap();
                            index += 1;
                            v
                        })
                        .collect::<Vec<_>>();
                    self.pack(builder, ty, pieces, &values)?
                }
            };
            args.push(value);
        }
        Ok(args)
    }
    fn call_with(
        &self,
        builder: &Builder<'ctx>,
        abi: &FunctionAbi,
        function: Option<FunctionValue<'ctx>>,
        pointer: Option<PointerValue<'ctx>>,
        arguments: &[BasicValueEnum<'ctx>],
    ) -> Result<Option<BasicValueEnum<'ctx>>, BackendError> {
        if arguments.len() != abi.parameters.len() {
            return Err(error("ABI argument count mismatch"));
        }
        let mut physical = Vec::new();
        let mut result_pointer = None;
        if let Some(c) = &abi.c_abi {
            if matches!(c.result, Some(Boundary::Indirect { .. })) {
                let p = self.scratch(builder, self.types[abi.result.unwrap().0])?;
                physical.push(p.into());
                result_pointer = Some(p);
            }
            for (value, boundary) in arguments.iter().zip(&c.arguments) {
                match boundary {
                    Boundary::Direct => physical.push((*value).into()),
                    Boundary::Indirect { .. } => {
                        let p = self.scratch(builder, value.get_type())?;
                        builder.build_store(p, *value).map_err(error)?;
                        physical.push(p.into());
                    }
                    Boundary::Coerce(pieces) => physical.extend(
                        self.unpack(builder, *value, pieces)?
                            .into_iter()
                            .map(BasicMetadataValueEnum::from),
                    ),
                }
            }
        } else {
            physical = vec![self.context.i8_type().const_zero().into(); arguments.len()];
            for (value, position) in arguments.iter().zip(&abi.parameter_positions) {
                physical[*position] = (*value).into();
            }
        }
        let name = if self.result_type(abi).is_some() {
            "call"
        } else {
            ""
        };
        let call = match function {
            Some(f) => builder.build_call(f, &physical, name),
            None => builder.build_indirect_call(
                self.function_type(abi),
                pointer.unwrap(),
                &physical,
                name,
            ),
        }
        .map_err(error)?;
        call.set_call_convention(convention(abi.convention));
        self.call_attributes(call, abi);
        let Some(ty) = abi.result else {
            return Ok(None);
        };
        if let Some(p) = result_pointer {
            return builder
                .build_load(self.types[ty.0], p, "abi.result")
                .map(Some)
                .map_err(error);
        }
        let value = call
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| error("missing ABI result"))?;
        if let Some(Boundary::Coerce(pieces)) = abi.c_abi.as_ref().and_then(|c| c.result.as_ref()) {
            let values = if pieces.len() == 1 {
                vec![value]
            } else {
                (0..pieces.len())
                    .map(|i| {
                        builder
                            .build_extract_value(
                                value.into_struct_value(),
                                i as u32,
                                "abi.result.part",
                            )
                            .map_err(error)
                    })
                    .collect::<Result<Vec<_>, _>>()?
            };
            return self.pack(builder, ty, pieces, &values).map(Some);
        }
        Ok(Some(value))
    }
    fn call_attributes(&self, call: CallSiteValue<'ctx>, abi: &FunctionAbi) {
        for (location, attribute) in self.attributes(abi) {
            call.add_attribute(location, attribute);
        }
    }
    pub fn call(
        &self,
        builder: &Builder<'ctx>,
        id: FunctionId,
        function: FunctionValue<'ctx>,
        args: &[BasicValueEnum<'ctx>],
    ) -> Result<Option<BasicValueEnum<'ctx>>, BackendError> {
        self.call_with(builder, self.plan.function(id)?, Some(function), None, args)
    }
    pub fn call_indirect(
        &self,
        builder: &Builder<'ctx>,
        signature: &Signature,
        pointer: PointerValue<'ctx>,
        args: &[BasicValueEnum<'ctx>],
    ) -> Result<Option<BasicValueEnum<'ctx>>, BackendError> {
        let abi = super::abi::classify_signature(signature, &self.ir.types, self.target)?;
        self.call_with(builder, &abi, None, Some(pointer), args)
    }
    pub fn return_value(
        &self,
        builder: &Builder<'ctx>,
        id: FunctionId,
        value: Option<BasicValueEnum<'ctx>>,
    ) -> Result<(), BackendError> {
        let abi = self.plan.function(id)?;
        if abi.result.is_some() != value.is_some() {
            return Err(error("ABI return count mismatch"));
        }
        let mut value = value;
        match abi.c_abi.as_ref().and_then(|c| c.result.as_ref()) {
            Some(Boundary::Indirect { .. }) => {
                let function = builder
                    .get_insert_block()
                    .and_then(|b| b.get_parent())
                    .ok_or_else(|| error("missing function"))?;
                builder
                    .build_store(
                        function.get_nth_param(0).unwrap().into_pointer_value(),
                        value.unwrap(),
                    )
                    .map_err(error)?;
                value = None;
            }
            Some(Boundary::Coerce(pieces)) => {
                let parts = self.unpack(builder, value.unwrap(), pieces)?;
                value = Some(if parts.len() == 1 {
                    parts[0]
                } else {
                    let mut aggregate = self
                        .result_type(abi)
                        .unwrap()
                        .into_struct_type()
                        .const_zero();
                    for (i, part) in parts.into_iter().enumerate() {
                        aggregate = builder
                            .build_insert_value(aggregate, part, i as u32, "abi.return")
                            .map_err(error)?
                            .into_struct_value();
                    }
                    aggregate.into()
                });
            }
            _ => {}
        }
        builder
            .build_return(value.as_ref().map(|v| v as &dyn BasicValue))
            .map_err(error)?;
        Ok(())
    }
}
fn convention(c: PhysicalConvention) -> u32 {
    match c {
        PhysicalConvention::C => 0,
        PhysicalConvention::FibDirect => 8,
    }
}
fn error(message: impl ToString) -> BackendError {
    BackendError::Invalid {
        message: message.to_string(),
    }
}
