use crate::frontend::{
    next_ast::expression::Expression,
    tokens::{Operator, Token},
};

use super::Parser;

impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    pub fn parse_bitwise_and(&mut self) -> anyhow::Result<Expression> {
        self.parse_binary(&[Operator::Ampersand], Self::parse_equality)
    }
}
