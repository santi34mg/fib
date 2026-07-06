use crate::frontend::{ast::{function_parameter::FunctionParameter, type_expression::TypeExpression}, identifier::Identifier};

#[derive(Debug, Clone)]
pub struct FunctionSignature {
    pub name: Identifier,
    pub parameters: Vec<FunctionParameter>,
    pub return_type: Option<TypeExpression>,
}

