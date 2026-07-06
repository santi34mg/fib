use crate::frontend::{
    ast::expression::Expression,
    parser::ParseResult,
    tokens::{Operator, Token, TokenKind},
};

use super::Parser;

impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    pub fn parse_shift(&mut self) -> ParseResult<Expression> {
        let mut expr = self.parse_additive()?;
        while let Some(token) = self.peek() {
            match &token.kind {
                TokenKind::Operator(op @ (Operator::LeftShift | Operator::RightShift)) => {
                    let op = *op;
                    self.next();
                    let right = Box::new(self.parse_additive()?);
                    expr = Expression::Binary {
                        left: Box::new(expr),
                        operator: op,
                        right,
                    };
                }
                _ => break,
            }
        }
        Ok(expr)
    }
}
