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
    pub fn parse_unary(&mut self) -> ParseResult<Expression> {
        if let Some(token) = self.peek() {
            match &token.kind {
                TokenKind::Operator(Operator::LogicalNot) => {
                    self.next();
                    let expr = self.parse_unary()?;
                    Ok(Expression::Unary {
                        operator: Operator::LogicalNot,
                        expression: Box::new(expr),
                    })
                }
                TokenKind::Operator(Operator::Minus) => {
                    self.next();
                    let expr = self.parse_unary()?;
                    Ok(Expression::Unary {
                        operator: Operator::Minus,
                        expression: Box::new(expr),
                    })
                }
                TokenKind::Operator(Operator::Tilde) => {
                    self.next();
                    let expr = self.parse_unary()?;
                    Ok(Expression::Unary {
                        operator: Operator::Tilde,
                        expression: Box::new(expr),
                    })
                }
                _ => self.parse_atom(),
            }
        } else {
            self.parse_atom()
        }
    }
}
