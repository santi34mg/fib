use crate::frontend::{ast::expression::Expression, parser::ParseResult, tokens::Operator};

use super::Parser;

impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = crate::frontend::tokens::Token>,
{
    /// Parse equality and comparison expressions (==, !=, >, <, >=, <=)
    pub fn parse_equality(&mut self) -> ParseResult<Expression> {
        self.parse_binary(
            &[Operator::DoubleEquals, Operator::Different],
            Self::parse_comparison,
        )
    }
}
