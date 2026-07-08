use crate::frontend::{ast::type_expression::TypeExpression, identifier::Identifier};

#[derive(Debug, Clone)]
pub struct FunctionParameter {
    pub parameter_name: Identifier,
    pub parameter_type: TypeExpression,
}
