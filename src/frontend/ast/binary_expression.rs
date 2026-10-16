use crate::frontend::{ast::expression::Expression, tokens::Operator};

#[derive(Debug, Clone, PartialEq)]
pub struct BinaryExpression {
    pub left: Box<Expression>,
    pub operator: Operator,
    pub right: Box<Expression>,
}
