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
    pub fn parse_comparison(&mut self) -> ParseResult<Expression> {
        let mut expr = self.parse_cast()?;

        while let Some(token) = self.peek() {
            match &token.kind {
                TokenKind::Operator(
                    op @ (Operator::GreaterThan
                    | Operator::LesserThan
                    | Operator::GreaterEqual
                    | Operator::LesserEqual),
                ) => {
                    let op = *op;
                    self.next();
                    let right = Box::new(self.parse_cast()?);
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
