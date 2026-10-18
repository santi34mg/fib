use crate::frontend::ast::{function_body::FunctionBody, function_signature::FunctionSignature};

#[derive(Debug, Clone)]
pub struct FunctionDeclaration {
    pub signature: FunctionSignature,
    pub body: Option<FunctionBody>,
    pub is_extern: bool,
    pub is_variadic: bool,
    pub is_pub: bool,
}

impl FunctionDeclaration {
    pub fn is_generic(&self) -> bool {
        !self.signature.generic_parameters.is_empty()
    }
}
