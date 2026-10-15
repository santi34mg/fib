use crate::frontend::{
    next_ast::{
        binary_expression::BinaryExpression,
        expression::{Expression, ExpressionKind},
    },
    tokens::{Operator, Token},
};

use super::Parser;
use crate::frontend::tokens::TokenKind;

impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    /// Parse one left-associative binary-operator precedence level.
    ///
    /// `ops` lists the operators handled at this level; `operand` parses the
    /// next-higher precedence level. A plain `fn` pointer is used instead of a
    /// closure so the call graph stays greppable (`Self::parse_term`, ...).
    /// Error behavior is identical to the old per-level loops: a missing
    /// right-hand side propagates the operand parser's error.
    pub fn parse_binary(
        &mut self,
        ops: &[Operator],
        operand: fn(&mut Self) -> anyhow::Result<Expression>,
    ) -> anyhow::Result<Expression> {
        let mut expr = operand(self)?;
        while let Some(token) = self.peek() {
            match &token.kind {
                TokenKind::Operator(op) if ops.contains(op) => {
                    let op = *op;
                    self.next();
                    let right = Box::new(operand(self)?);
                    let span = expr.span;
                    expr = Expression::at(
                        ExpressionKind::BinaryExpresison(BinaryExpression {
                            left: Box::new(expr),
                            operator: op,
                            right,
                        }),
                        span,
                    );
                }
                _ => break,
            }
        }
        Ok(expr)
    }
}
