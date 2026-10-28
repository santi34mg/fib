//! Per-function ABI classification. No LLVM objects or compilation-wide ABI choice.
use super::BackendError;
use crate::ir::{CallingConvention, FunctionId, IntegerWidth, Ir, IrType, Signature, TypeId};

/// Resolved target information supplied by the LLVM adapter, not guessed layout.
#[derive(Debug, Clone)]
pub struct AbiTarget {
    pub triple: String,
    pub data_layout: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhysicalConvention {
    C,
    FibDirect,
}

/// Direct value boundary mapping. Positions map logical arguments to physical
/// parameters. Wider legalization needs explicit additional representations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionAbi {
    pub c_abi: Option<super::c_abi::CSignature>,
    pub parameters: Vec<TypeId>,
    pub parameter_positions: Vec<usize>,
    pub result: Option<TypeId>,
    pub convention: PhysicalConvention,
}

impl FunctionAbi {
    pub fn validate(&self, signature: &Signature, types: &[IrType]) -> Result<(), BackendError> {
        let invalid = || failure("invalid direct ABI mapping");
        match (signature.calling_convention, self.convention, &self.c_abi) {
            (CallingConvention::C, PhysicalConvention::C, Some(c))
                if c.arguments.len() == signature.params.len()
                    && c.result.is_none() == signature.results.is_empty() => {}
            (CallingConvention::Fib, PhysicalConvention::FibDirect, None) => {}
            _ => return Err(failure("ABI convention or boundary plan mismatch")),
        }
        if signature.results.len() > 1
            || self.parameters.len() != signature.params.len()
            || self.parameter_positions.len() != signature.params.len()
            || self.result != signature.results.first().copied()
        {
            return Err(invalid());
        }
        let mut positions = std::collections::HashSet::new();
        for (&logical, &position) in signature.params.iter().zip(&self.parameter_positions) {
            if self.parameters.get(position) != Some(&logical) || !positions.insert(position) {
                return Err(invalid());
            }
        }
        for &ty in self.parameters.iter().chain(self.result.iter()) {
            if !direct_type(ty, types, &mut std::collections::HashSet::new()) {
                return Err(failure("invalid direct Fib ABI type"));
            }
        }
        Ok(())
    }
}

/// Implementations classify one signature for one resolved target. Dispatch is
/// selected by the signature's convention, never by an emitter entry point.
pub trait AbiClassifier {
    fn classify(
        &self,
        signature: &Signature,
        types: &[IrType],
        target: &AbiTarget,
    ) -> Result<FunctionAbi, BackendError>;
}

struct CScalar;
impl AbiClassifier for CScalar {
    fn classify(
        &self,
        signature: &Signature,
        types: &[IrType],
        target: &AbiTarget,
    ) -> Result<FunctionAbi, BackendError> {
        if signature.calling_convention != CallingConvention::C {
            return Err(failure("unsupported ABI convention"));
        }
        if target.triple.is_empty() || target.data_layout.is_empty() {
            return Err(failure("ABI classification requires a resolved target"));
        }
        let abi = FunctionAbi {
            c_abi: Some(super::c_abi::classify(signature, types, target)?),
            parameters: signature.params.clone(),
            parameter_positions: (0..signature.params.len()).collect(),
            result: signature.results.first().copied(),
            convention: PhysicalConvention::C,
        };
        abi.validate(signature, types)?;
        Ok(abi)
    }
}

pub fn classify_signature(
    signature: &Signature,
    types: &[IrType],
    target: &AbiTarget,
) -> Result<FunctionAbi, BackendError> {
    match signature.calling_convention {
        CallingConvention::C => CScalar.classify(signature, types, target),
        CallingConvention::Fib => {
            if target.triple.is_empty() || target.data_layout.is_empty() {
                return Err(failure("ABI classification requires a resolved target"));
            }
            let abi = FunctionAbi {
                c_abi: None,
                parameters: signature.params.clone(),
                parameter_positions: (0..signature.params.len()).collect(),
                result: signature.results.first().copied(),
                convention: PhysicalConvention::FibDirect,
            };
            abi.validate(signature, types)?;
            Ok(abi)
        }
    }
}

#[derive(Debug)]
pub struct AbiPlan {
    functions: Vec<FunctionAbi>,
}
impl AbiPlan {
    pub fn function(&self, function: FunctionId) -> Result<&FunctionAbi, BackendError> {
        self.functions
            .get(function.0)
            .ok_or_else(|| failure("missing function ABI"))
    }
}

pub fn classify_abis(ir: &Ir, target: &AbiTarget) -> Result<AbiPlan, BackendError> {
    classify_with(ir, target, classify_signature)
}

// Private dispatch seam also lets tests exercise mixed conventions without
// exposing policy injection or assigning a production meaning to Fib ABI.
pub(super) fn classify_with(
    ir: &Ir,
    target: &AbiTarget,
    classify: impl Fn(&Signature, &[IrType], &AbiTarget) -> Result<FunctionAbi, BackendError>,
) -> Result<AbiPlan, BackendError> {
    let functions = ir
        .functions
        .iter()
        .map(|f| {
            let abi = classify(&f.signature, &ir.types, target)?;
            abi.validate(&f.signature, &ir.types)?;
            Ok(abi)
        })
        .collect::<Result<_, BackendError>>()?;
    Ok(AbiPlan { functions })
}

fn failure(message: &str) -> BackendError {
    BackendError::Invalid {
        message: message.into(),
    }
}

#[cfg(test)]
mod test;

fn direct_type(
    ty: TypeId,
    types: &[IrType],
    active: &mut std::collections::HashSet<TypeId>,
) -> bool {
    match types.get(ty.0) {
        Some(
            IrType::Bool
            | IrType::Float(_)
            | IrType::Integer {
                width: IntegerWidth::Bits(1..=128) | IntegerWidth::Pointer,
                ..
            },
        ) => true,
        Some(IrType::FunctionPointer(_)) => true,
        Some(IrType::Pointer { pointee }) => types.get(pointee.0).is_some(),
        Some(IrType::Slice { element }) => direct_type(*element, types, active),
        Some(IrType::Tuple(fields)) => {
            if !active.insert(ty) {
                return false;
            }
            let valid = fields.iter().all(|t| direct_type(*t, types, active));
            active.remove(&ty);
            valid
        }
        Some(IrType::Array { element, .. }) => {
            if !active.insert(ty) {
                return false;
            }
            let valid = direct_type(*element, types, active);
            active.remove(&ty);
            valid
        }
        Some(IrType::Struct { fields, .. }) => {
            if !active.insert(ty) {
                return false;
            }
            let valid = fields
                .iter()
                .all(|field| direct_type(*field, types, active));
            active.remove(&ty);
            valid
        }
        Some(IrType::Enum { variants, .. }) => {
            if !active.insert(ty) {
                return false;
            }
            let valid = variants
                .iter()
                .flat_map(|v| &v.fields)
                .all(|f| direct_type(*f, types, active));
            active.remove(&ty);
            valid
        }
        _ => false,
    }
}
