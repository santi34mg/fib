use crate::frontend::{ast::type_expression::TypeExpression, identifier::Identifier};

#[derive(Debug, Clone)]
pub struct TypeDeclaration {
    pub name: Identifier,
    pub expression: TypeExpression,
}
