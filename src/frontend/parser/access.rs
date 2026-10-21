use anyhow::bail;

use crate::frontend::{
    ast::{
        expression::{Expression, ExpressionKind},
        unary_expression::UnaryExpression,
    },
    tokens::{Operator, Punctuation, Token, TokenKind},
};
const DOT_ACCESS_HELP: &str = "expected one of
* field access:                 `expr.field`
* enum variant construction:    `expr.variant payload`
* dereference:                  `expr*`
* address-of:                   `expr&`
* index access:                 `expr[a]`
* slice:                        `expr[a..b]`
";

// a :+= Color.red 20

use super::Parser;

impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    /// Parse postfix operations after a primary expression:
    /// * function calls:               `expr(args)`
    /// * expression access:            `expr.field`
    /// * dereference:                  `expr*`
    /// * address-of:                   `expr&`
    /// * index access:                 `expr[i]`
    /// * slice:                        `expr[a..b]`
    pub fn parse_postfix(&mut self, mut expr: Expression) -> anyhow::Result<Expression> {
        while let Some(token) = self.peek() {
            expr = match token.kind {
                TokenKind::Punctuation(Punctuation::OpeningParenthesis) => self.parse_call(expr)?,
                TokenKind::Punctuation(Punctuation::Dot) => self.parse_dot(expr)?,
                TokenKind::Punctuation(Punctuation::OpeningSquareBrace) => {
                    let span = expr.span;
                    Expression::at(self.parse_slice_or_index(expr)?, span)
                }
                TokenKind::Operator(operator @ (Operator::Star | Operator::Ampersand))
                    if matches!(
                        self.peek_second().map(|token| token.kind),
                        Some(TokenKind::Punctuation(
                            Punctuation::Semicolon
                                | Punctuation::ClosingParenthesis
                                | Punctuation::ClosingSquareBrace
                                | Punctuation::ClosingCurlyBrace
                                | Punctuation::Comma
                        )) | Some(TokenKind::Operator(Operator::Assign))
                    ) =>
                {
                    self.next();
                    let span = expr.span;
                    Expression::at(
                        ExpressionKind::UnaryExpression(UnaryExpression {
                            operator: if operator == Operator::Star {
                                Operator::Dereference
                            } else {
                                Operator::AddressOf
                            },
                            expression: Box::new(expr),
                        }),
                        span,
                    )
                }
                _ => break,
            };
        }
        Ok(expr)
    }

    fn parse_call(&mut self, expr: Expression) -> anyhow::Result<Expression> {
        self.expect_token(
            TokenKind::Punctuation(Punctuation::OpeningParenthesis),
            "expected '('",
        )?;
        let args = self.parse_call_arguments("function call arguments")?;
        let span = expr.span;
        Ok(Expression::at(
            ExpressionKind::Call {
                callee: Box::new(expr),
                args,
            },
            span,
        ))
    }

    fn parse_dot(&mut self, expr: Expression) -> anyhow::Result<Expression> {
        self.expect_token(TokenKind::Punctuation(Punctuation::Dot), "expected '.'")?;
        if let Some(token) = self.peek() {
            let span = expr.span;
            match token.kind {
                TokenKind::Identifier(identifier_string) => {
                    self.next();
                    Ok(Expression::at(
                        ExpressionKind::ExprAcess {
                            expr: Box::new(expr),
                            accessed_identifier: identifier_string,
                            member: Default::default(),
                        },
                        span,
                    ))
                }
                // TODO: .literal for the future
                // like expr.1 for tuples?
                TokenKind::Literal(_) => Err(self.error(
                    "tuple index access is not implemented",
                    token.line,
                    token.column,
                )),
                TokenKind::Error(message) => Err(self.error(&message, token.line, token.column)),
                _ => bail!(DOT_ACCESS_HELP),
            }
        } else {
            bail!(DOT_ACCESS_HELP)
        }
    }

    fn parse_slice_or_index(&mut self, expr: Expression) -> anyhow::Result<ExpressionKind> {
        // * index access: `expr[a]`
        // * slice: `expr[a..b]`
        self.expect_token(
            TokenKind::Punctuation(Punctuation::OpeningSquareBrace),
            "expected '['",
        )?;
        if let Some(t) = self.peek() {
            match t.kind {
                // `[..]` or `[..b]`
                TokenKind::Operator(Operator::DoubleDot) => {
                    self.next();
                    self.parse_slice_no_start(expr, false)
                }
                // `[.=]` or `[.=b]`
                TokenKind::Operator(Operator::DotEq) => {
                    self.next();
                    self.parse_slice_no_start(expr, true)
                }
                _ => {
                    // `[a` starts an index or a slice.
                    let start = self.parse_expression()?;
                    if let Some(t) = self.peek() {
                        match t.kind {
                            // `[a]`
                            TokenKind::Punctuation(Punctuation::ClosingSquareBrace) => {
                                self.next(); // consume ']'
                                Ok(ExpressionKind::IndexAccess {
                                    object: Box::new(expr),
                                    index: Box::new(start),
                                })
                            }
                            // `[a..`
                            TokenKind::Operator(Operator::DoubleDot) => {
                                self.next();
                                self.parse_slice_start(expr, start, false)
                            }
                            TokenKind::Operator(Operator::DotEq) => {
                                self.next();
                                self.parse_slice_start(expr, start, true)
                            }
                            _ => {
                                bail!("unexpected token found: {:?}", t)
                            }
                        }
                    } else {
                        bail!("expected a token, maybe missing ']'")
                    }
                }
            }
        } else {
            bail!("expected a token, a '[' in particular")
        }
    }

    fn parse_slice_no_start(
        &mut self,
        expr: Expression,
        inclusive: bool,
    ) -> anyhow::Result<ExpressionKind> {
        if let Some(Token {
            kind: TokenKind::Punctuation(Punctuation::ClosingSquareBrace),
            ..
        }) = self.peek()
        {
            if inclusive {
                return Err(self.error(
                    "inclusive slice requires an end bound",
                    self.last_pos.0,
                    self.last_pos.1,
                ));
            }
            // `expr[..]`, early return
            self.expect_token(
                TokenKind::Punctuation(Punctuation::ClosingSquareBrace),
                "expected ']'",
            )?;
            Ok(ExpressionKind::SliceLiteral {
                object: Box::new(expr),
                start: None,
                end: None,
                inclusive,
            })
        } else {
            // `expr[..b]`
            let end = self.parse_expression()?;
            self.expect_token(
                TokenKind::Punctuation(Punctuation::ClosingSquareBrace),
                "expected ']'",
            )?;
            Ok(ExpressionKind::SliceLiteral {
                object: Box::new(expr),
                start: None,
                end: Some(Box::new(end)),
                inclusive,
            })
        }
    }

    fn parse_slice_start(
        &mut self,
        expr: Expression,
        start: Expression,
        inclusive: bool,
    ) -> anyhow::Result<ExpressionKind> {
        if let Some(Token {
            kind: TokenKind::Punctuation(Punctuation::ClosingSquareBrace),
            ..
        }) = self.peek()
        {
            if inclusive {
                return Err(self.error(
                    "inclusive slice requires an end bound",
                    self.last_pos.0,
                    self.last_pos.1,
                ));
            }
            // `expr[a..]`
            self.expect_token(
                TokenKind::Punctuation(Punctuation::ClosingSquareBrace),
                "expected ']'",
            )?;
            Ok(ExpressionKind::SliceLiteral {
                object: Box::new(expr),
                start: Some(Box::new(start)),
                end: None,
                inclusive,
            })
        } else {
            // `expr[a..b]`
            let end = self.parse_expression()?;
            self.expect_token(
                TokenKind::Punctuation(Punctuation::ClosingSquareBrace),
                "expected ']'",
            )?; // consume ']'
            Ok(ExpressionKind::SliceLiteral {
                object: Box::new(expr),
                start: Some(Box::new(start)),
                end: Some(Box::new(end)),
                inclusive,
            })
        }
    }
}
