use crate::frontend::{identifier::Identifier, ast::type_expression::TypeExpression};

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub(crate) label: Identifier,
    pub(crate) type_id: TypeExpression,
}
