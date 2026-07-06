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
    pub fn parse_bitwise_and(&mut self) -> ParseResult<Expression> {
        let mut expr = self.parse_equality()?;
        while let Some(token) = self.peek() {
            match &token.kind {
                TokenKind::Operator(Operator::Ampersand) => {
                    self.next();
                    let right = Box::new(self.parse_equality()?);
                    expr = Expression::Binary {
                        left: Box::new(expr),
                        operator: Operator::Ampersand,
                        right,
                    };
                }
                _ => break,
            }
        }
        Ok(expr)
    }
}
