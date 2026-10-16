use crate::frontend::{
    ast::expression::Expression,
    tokens::{Operator, Token},
};

use super::Parser;

impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    pub fn parse_bitwise_or(&mut self) -> anyhow::Result<Expression> {
        self.parse_binary(&[Operator::Pipe], Self::parse_bitwise_xor)
    }
}
