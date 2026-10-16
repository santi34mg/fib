use crate::frontend::{
    next_ast::expression::Expression,
    tokens::{Operator, Token},
};

use super::Parser;

impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    /// Parse equality and comparison expressions (==, !=, >, <, >=, <=)
    pub fn parse_equality(&mut self) -> anyhow::Result<Expression> {
        self.parse_binary(
            &[Operator::DoubleEquals, Operator::Different],
            Self::parse_comparison,
        )
    }
}
