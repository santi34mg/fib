use crate::frontend::{
    ast::expression::{Expression, ExpressionKind},
    parser::ParseResult,
    tokens::Token,
};

use super::Parser;

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
        ops: &[crate::frontend::tokens::Operator],
        operand: fn(&mut Self) -> ParseResult<Expression>,
    ) -> ParseResult<Expression> {
        use crate::frontend::tokens::TokenKind;

        let mut expr = operand(self)?;
        while let Some(token) = self.peek() {
            match &token.kind {
                TokenKind::Operator(op) if ops.contains(op) => {
                    let op = *op;
                    self.next();
                    let right = Box::new(operand(self)?);
                    let span = expr.span;
                    expr = Expression::at(
                        ExpressionKind::Binary {
                            left: Box::new(expr),
                            operator: op,
                            right,
                        },
                        span,
                    );
                }
                _ => break,
            }
        }
        Ok(expr)
    }
}
