use crate::frontend::{
    ast::expression::Expression,
    tokens::{Operator, Token},
};

use super::Parser;

impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    pub fn parse_comparison(&mut self) -> anyhow::Result<Expression> {
        self.parse_binary(
            &[
                Operator::GreaterThan,
                Operator::LesserThan,
                Operator::GreaterEqual,
                Operator::LesserEqual,
            ],
            Self::parse_cast,
        )
    }
}
