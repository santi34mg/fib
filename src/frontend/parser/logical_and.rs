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
    pub fn parse_logical_and(&mut self) -> ParseResult<Expression> {
        let mut expr = self.parse_bitwise_or()?;
        while let Some(token) = self.peek() {
            match &token.kind {
                TokenKind::Operator(Operator::LogicalAnd) => {
                    self.next();
                    let right = Box::new(self.parse_bitwise_or()?);
                    expr = Expression::Binary {
                        left: Box::new(expr),
                        operator: Operator::LogicalAnd,
                        right,
                    };
                }
                _ => break,
            }
        }
        Ok(expr)
    }
}
