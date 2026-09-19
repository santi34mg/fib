use crate::diagnostics::Span;
use crate::frontend::{
    ast::expression::{Expression, ExpressionKind},
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
                    let span = Span::new(token.line, token.column);
                    let expr = self.parse_unary()?;
                    Ok(Expression::at(
                        ExpressionKind::Unary {
                            operator: Operator::LogicalNot,
                            expression: Box::new(expr),
                        },
                        span,
                    ))
                }
                TokenKind::Operator(Operator::Minus) => {
                    self.next();
                    let span = Span::new(token.line, token.column);
                    let expr = self.parse_unary()?;
                    Ok(Expression::at(
                        ExpressionKind::Unary {
                            operator: Operator::Minus,
                            expression: Box::new(expr),
                        },
                        span,
                    ))
                }
                TokenKind::Operator(Operator::Tilde) => {
                    self.next();
                    let span = Span::new(token.line, token.column);
                    let expr = self.parse_unary()?;
                    Ok(Expression::at(
                        ExpressionKind::Unary {
                            operator: Operator::Tilde,
                            expression: Box::new(expr),
                        },
                        span,
                    ))
                }
                _ => self.parse_atom(),
            }
        } else {
            self.parse_atom()
        }
    }
}
