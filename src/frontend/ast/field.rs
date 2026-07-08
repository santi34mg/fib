use crate::frontend::{ast::type_expression::TypeExpression, identifier::Identifier};

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub(crate) label: Identifier,
    pub(crate) type_id: TypeExpression,
}
