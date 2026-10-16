use crate::frontend::{
    ast::{function_parameter::FunctionParameter, type_expression::TypeExpression},
    tokens::name::Name,
};

#[derive(Debug, Clone)]
pub struct FunctionSignature {
    pub funcition_name_symbol: Name,
    pub parameters: Vec<FunctionParameter>,
    pub return_type: TypeExpression,
    pub generic_parameters: Vec<Name>,
}
