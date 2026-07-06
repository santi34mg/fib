use crate::frontend::{
    ast::expression::Expression,
    parser::ParseResult,
    tokens::{Keyword, Token, TokenKind},
};

use super::Parser;

impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    pub fn parse_cast(&mut self) -> ParseResult<Expression> {
        let mut expr = self.parse_shift()?;
        while self
            .consume_if(|t| matches!(t.kind, TokenKind::Keyword(Keyword::As)))
            .is_some()
        {
            let target_type = self.parse_type_expression()?.ok_or_else(|| {
                let (line, col) = self.peek().map_or((0, 0), |t| (t.line, t.column));
                self.error("expected type after 'as'", line, col)
            })?;
            expr = Expression::Cast {
                expr: Box::new(expr),
                target_type,
            };
        }
        Ok(expr)
    }
}
