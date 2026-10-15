use crate::frontend::{next_ast::expression::Expression, tokens::name::Name};

#[derive(Debug, Clone, PartialEq)]
pub struct StructField {
    pub field_name: Name,
    pub expression: Expression,
}
