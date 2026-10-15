use crate::frontend::{
    next_ast::expression::Expression,
    tokens::{Operator, Token},
};

use super::Parser;

impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    pub fn parse_logical_or(&mut self) -> anyhow::Result<Expression> {
        self.parse_binary(&[Operator::LogicalOr], Self::parse_logical_and)
    }
}
