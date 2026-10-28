use crate::frontend::ast::expression::Expression;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EnumVariant {
    pub identifier: String,
    pub payload: Option<Box<Expression>>,
}
