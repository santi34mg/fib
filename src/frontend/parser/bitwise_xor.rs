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
    pub fn parse_bitwise_xor(&mut self) -> ParseResult<Expression> {
        let mut expr = self.parse_bitwise_and()?;
        while let Some(token) = self.peek() {
            match &token.kind {
                TokenKind::Operator(Operator::Caret) => {
                    self.next();
                    let right = Box::new(self.parse_bitwise_and()?);
                    expr = Expression::Binary {
                        left: Box::new(expr),
                        operator: Operator::Caret,
                        right,
                    };
                }
                _ => break,
            }
        }
        Ok(expr)
    }
}
