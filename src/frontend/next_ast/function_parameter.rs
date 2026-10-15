use crate::frontend::{next_ast::type_expression::TypeExpression, tokens::name::Name};

#[derive(Debug, Clone)]
pub struct FunctionParameter {
    pub parameter_name: Name,
    pub parameter_type: TypeExpression,
}
