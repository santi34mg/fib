use crate::frontend::{ast::expression::Expression, parser::ParseResult, tokens::Token};

use super::Parser;

impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    /// Parse an atomic expression: a primary followed by any postfix
    /// operators. See [`Parser::parse_primary`] and
    /// [`Parser::parse_postfix`] for the two halves.
    pub fn parse_atom(&mut self) -> ParseResult<Expression> {
        let expr = self.parse_primary()?;
        self.parse_postfix(expr)
    }
}
