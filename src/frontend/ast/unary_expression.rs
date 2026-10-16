use crate::frontend::{ast::expression::Expression, tokens::Operator};

#[derive(Debug, Clone, PartialEq)]
pub struct UnaryExpression {
    pub operator: Operator,
    pub expression: Box<Expression>,
}
