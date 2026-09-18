use crate::frontend::{ast::expression::Expression, parser::ParseResult, tokens::Operator};

use super::Parser;

impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = crate::frontend::tokens::Token>,
{
    pub fn parse_shift(&mut self) -> ParseResult<Expression> {
        self.parse_binary(
            &[Operator::LeftShift, Operator::RightShift],
            Self::parse_additive,
        )
    }
}
