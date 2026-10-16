use crate::frontend::{ast::type_expression::TypeExpression, tokens::name::Name};

#[derive(Debug, Clone)]
pub struct TypeDeclaration {
    pub identifier: Name,
    pub expression: TypeExpression,
}
