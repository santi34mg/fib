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
    pub fn parse_bitwise_or(&mut self) -> ParseResult<Expression> {
        let mut expr = self.parse_bitwise_xor()?;
        while let Some(token) = self.peek() {
            match &token.kind {
                TokenKind::Operator(Operator::Pipe) => {
                    self.next();
                    let right = Box::new(self.parse_bitwise_xor()?);
                    expr = Expression::Binary {
                        left: Box::new(expr),
                        operator: Operator::Pipe,
                        right,
                    };
                }
                _ => break,
            }
        }
        Ok(expr)
    }
}
