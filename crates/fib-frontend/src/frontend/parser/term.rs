use crate::frontend::{
    ast::expression::Expression,
    tokens::{Operator, Token},
};

use super::Parser;

impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    pub fn parse_term(&mut self) -> anyhow::Result<Expression> {
        self.parse_binary(
            &[Operator::Star, Operator::Slash, Operator::Percent],
            Self::parse_unary,
        )
    }
}
