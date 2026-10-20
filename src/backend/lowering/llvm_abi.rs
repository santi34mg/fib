//! LLVM adapter for classified function boundaries; CFG values remain logical.
use super::{
    BackendError,
    abi::{AbiPlan, PhysicalConvention},
};
use crate::ir::{Function, FunctionId, Linkage};
use inkwell::{
    builder::Builder,
    context::Context,
    module::{Linkage as LlvmLinkage, Module},
    types::IntType,
    values::{BasicValue, FunctionValue, IntValue},
};

pub(super) struct LlvmAbi<'a, 'ctx> {
    pub context: &'ctx Context,
    pub plan: &'a AbiPlan,
    pub types: &'a [IntType<'ctx>],
}
impl<'ctx> LlvmAbi<'_, 'ctx> {
    pub fn declare(
        &self,
        module: &Module<'ctx>,
        id: FunctionId,
        function: &Function,
    ) -> Result<FunctionValue<'ctx>, BackendError> {
        let abi = self.plan.function(id)?;
        let params: Vec<_> = abi
            .parameters
            .iter()
            .map(|ty| self.types[ty.0].into())
            .collect();
        let signature = match abi.result {
            Some(ty) => self.types[ty.0].fn_type(&params, false),
            None => self.context.void_type().fn_type(&params, false),
        };
        let linkage = match function.linkage {
            Linkage::Internal => LlvmLinkage::Internal,
            Linkage::External => LlvmLinkage::External,
        };
        let value = module.add_function(&function.name, signature, Some(linkage));
        value.set_call_conventions(convention(abi.convention));
        Ok(value)
    }
    pub fn arguments(
        &self,
        id: FunctionId,
        function: FunctionValue<'ctx>,
    ) -> Result<Vec<IntValue<'ctx>>, BackendError> {
        self.plan
            .function(id)?
            .parameter_positions
            .iter()
            .map(|&position| {
                function
                    .get_nth_param(position as u32)
                    .map(|v| v.into_int_value())
                    .ok_or_else(|| error("missing physical argument"))
            })
            .collect()
    }
    pub fn call(
        &self,
        builder: &Builder<'ctx>,
        id: FunctionId,
        function: FunctionValue<'ctx>,
        arguments: &[IntValue<'ctx>],
    ) -> Result<Option<IntValue<'ctx>>, BackendError> {
        let abi = self.plan.function(id)?;
        if arguments.len() != abi.parameter_positions.len() {
            return Err(error("ABI argument count mismatch"));
        }
        let mut physical = vec![None; arguments.len()];
        for (&value, &position) in arguments.iter().zip(&abi.parameter_positions) {
            physical[position] = Some(value.into());
        }
        let physical = physical
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| error("incomplete ABI argument mapping"))?;
        let call = builder
            .build_call(
                function,
                &physical,
                if abi.result.is_some() { "call" } else { "" },
            )
            .map_err(error)?;
        call.set_call_convention(convention(abi.convention));
        match abi.result {
            Some(_) => Ok(Some(
                call.try_as_basic_value()
                    .basic()
                    .ok_or_else(|| error("missing ABI call result"))?
                    .into_int_value(),
            )),
            None => Ok(None),
        }
    }
    pub fn return_value(
        &self,
        builder: &Builder<'ctx>,
        id: FunctionId,
        value: Option<IntValue<'ctx>>,
    ) -> Result<(), BackendError> {
        let abi = self.plan.function(id)?;
        if abi.result.is_some() != value.is_some() {
            return Err(error("ABI return count mismatch"));
        }
        builder
            .build_return(value.as_ref().map(|v| v as &dyn BasicValue))
            .map_err(error)?;
        Ok(())
    }
}
fn convention(convention: PhysicalConvention) -> u32 {
    match convention {
        PhysicalConvention::C => 0,
    }
}
fn error(message: impl ToString) -> BackendError {
    BackendError::Invalid {
        message: message.to_string(),
    }
}
