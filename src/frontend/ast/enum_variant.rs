use crate::frontend::{ast::field::Field, identifier::Identifier};

#[derive(Debug, Clone, PartialEq)]
pub struct EnumVariant {
    pub name: Identifier,
    pub payload: Option<Vec<Field>>,
}
