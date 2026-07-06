use crate::frontend::{identifier::Identifier, ast::type_expression::TypeExpression};

#[derive(Debug, Clone)]
pub struct FunctionParameter {
    pub parameter_name: Identifier,
    pub parameter_type: TypeExpression,
}
