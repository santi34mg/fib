use crate::frontend::{ast::expression::Expression, parser::ParseResult, tokens::Operator};

use super::Parser;

impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = crate::frontend::tokens::Token>,
{
    pub fn parse_logical_or(&mut self) -> ParseResult<Expression> {
        self.parse_binary(&[Operator::LogicalOr], Self::parse_logical_and)
    }
}
