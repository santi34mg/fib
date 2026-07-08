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
    /// Parse equality and comparison expressions (==, !=, >, <, >=, <=)
    pub fn parse_equality(&mut self) -> ParseResult<Expression> {
        let mut expr = self.parse_comparison()?;

        while let Some(token) = self.peek() {
            match &token.kind {
                TokenKind::Operator(op @ (Operator::DoubleEquals | Operator::Different)) => {
                    let op = *op;
                    self.next();
                    let right = Box::new(self.parse_comparison()?);
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
