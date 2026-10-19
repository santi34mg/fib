use crate::frontend::{ast::expression::Expression, tokens::Operator};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UnaryExpression {
    pub operator: Operator,
    pub expression: Box<Expression>,
}
