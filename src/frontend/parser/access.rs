use crate::frontend::{
    ast::expression::{Expression, ExpressionKind},
    parser::ParseResult,
    tokens::{Operator, Punctuation, Token, TokenKind, builtin::Builtin},
};

use super::Parser;

impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    /// Parse postfix operations after a primary expression: function calls
    /// (`f(args)`), field access (`.field`), dereference (`.*`),
    /// address-of (`.&`), index access (`.[i]`), slice (`.[a..b]`), and
    /// enum variant construction (`Type.Variant { ... }`).
    pub fn parse_postfix(&mut self, mut expr: Expression) -> ParseResult<Expression> {
        while let Some(token) = self.peek() {
            if matches!(
                token.kind,
                TokenKind::Punctuation(Punctuation::OpeningParenthesis)
            ) {
                self.next(); // consume '('
                let args = self.parse_call_arguments("function call arguments")?;
                let span = expr.span;
                expr = Expression::at(
                    ExpressionKind::Call {
                        callee: Box::new(expr),
                        args,
                    },
                    span,
                );
            } else if matches!(token.kind, TokenKind::Punctuation(Punctuation::Dot)) {
                // Inside slice bounds, `.=` is the inclusive range
                // separator (`arr.[a.=b]` alias for `arr.[a..=b]`), not
                // field access — leave it for the slice parser.
                if self.slice_bound
                    && matches!(
                        self.peek_second(),
                        Some(Token {
                            kind: TokenKind::Operator(Operator::Assign),
                            ..
                        })
                    )
                {
                    break;
                }
                self.next(); // consume '.'
                // Check for `.[ index ]` before consuming
                if matches!(
                    self.peek(),
                    Some(Token {
                        kind: TokenKind::Punctuation(Punctuation::OpeningSquareBrace),
                        ..
                    })
                ) {
                    self.next(); // consume '['
                    // Helpers over the token stream for range separators.
                    fn is_double_dot(t: &Option<Token>) -> bool {
                        matches!(
                            t,
                            Some(Token {
                                kind: TokenKind::Operator(Operator::DoubleDot),
                                ..
                            })
                        )
                    }
                    fn is_assign(t: &Option<Token>) -> bool {
                        matches!(
                            t,
                            Some(Token {
                                kind: TokenKind::Operator(Operator::Assign),
                                ..
                            })
                        )
                    }
                    fn is_dot(t: &Option<Token>) -> bool {
                        matches!(
                            t,
                            Some(Token {
                                kind: TokenKind::Punctuation(Punctuation::Dot),
                                ..
                            })
                        )
                    }
                    fn is_close(t: &Option<Token>) -> bool {
                        matches!(
                            t,
                            Some(Token {
                                kind: TokenKind::Punctuation(Punctuation::ClosingSquareBrace),
                                ..
                            })
                        )
                    }
                    // Start-omitted: `[..b]`, `[.=b]` (inclusive), `[..]`.
                    if is_double_dot(&self.peek()) {
                        self.next(); // consume '..'
                        if is_assign(&self.peek()) {
                            let t = self.peek().expect("peeked '='");
                            return Err(self.error(
                                "parse_atom: use '.=' for inclusive ends, e.g. 'arr.[.=b]', not '..='",
                                t.line,
                                t.column,
                            ));
                        }
                        if is_close(&self.peek()) {
                            self.next(); // consume ']'
                            let span = expr.span;
                            expr = Expression::at(
                                ExpressionKind::Slice {
                                    object: Box::new(expr),
                                    start: None,
                                    end: None,
                                    inclusive: false,
                                },
                                span,
                            );
                        } else {
                            let end = self.parse_slice_bound()?;
                            self.expect_token(
                                TokenKind::Punctuation(Punctuation::ClosingSquareBrace),
                                "parse_atom: expected ']' after slice end expression",
                            )?;
                            let span = expr.span;
                            expr = Expression::at(
                                ExpressionKind::Slice {
                                    object: Box::new(expr),
                                    start: None,
                                    end: Some(Box::new(end)),
                                    inclusive: false,
                                },
                                span,
                            );
                        }
                    } else if is_dot(&self.peek()) && is_assign(&self.peek_second()) {
                        // `[.=b]` — inclusive end.
                        self.next(); // consume '.'
                        self.next(); // consume '='
                        if is_close(&self.peek()) {
                            let t = self.peek().expect("peeked ']'");
                            return Err(self.error(
                                "parse_atom: '.=' needs an end bound, e.g. 'arr.[.=b]'",
                                t.line,
                                t.column,
                            ));
                        }
                        let end = self.parse_slice_bound()?;
                        self.expect_token(
                            TokenKind::Punctuation(Punctuation::ClosingSquareBrace),
                            "parse_atom: expected ']' after slice end expression",
                        )?;
                        let span = expr.span;
                        expr = Expression::at(
                            ExpressionKind::Slice {
                                object: Box::new(expr),
                                start: None,
                                end: Some(Box::new(end)),
                                inclusive: true,
                            },
                            span,
                        );
                    } else if is_close(&self.peek()) {
                        let t = self.peek().expect("peeked ']'");
                        return Err(self.error(
                            "parse_atom: expected index or range inside '.[ ]', e.g. 'arr.[i]' or 'arr.[a..b]'",
                            t.line,
                            t.column,
                        ));
                    } else {
                        let first = self.parse_slice_bound()?;
                        // `.[a..b]` / `.[a.=b]` (inclusive) / `.[a..]`.
                        // Otherwise `.[i]` indexes.
                        if is_double_dot(&self.peek()) {
                            self.next(); // consume '..'
                            if is_assign(&self.peek()) {
                                let t = self.peek().expect("peeked '='");
                                return Err(self.error(
                                    "parse_atom: use '.=' for inclusive ends, e.g. 'arr.[a.=b]', not '..='",
                                    t.line,
                                    t.column,
                                ));
                            }
                            if is_close(&self.peek()) {
                                self.next(); // consume ']'
                                let span = expr.span;
                                expr = Expression::at(
                                    ExpressionKind::Slice {
                                        object: Box::new(expr),
                                        start: Some(Box::new(first)),
                                        end: None,
                                        inclusive: false,
                                    },
                                    span,
                                );
                            } else {
                                let end = self.parse_slice_bound()?;
                                self.expect_token(
                                    TokenKind::Punctuation(Punctuation::ClosingSquareBrace),
                                    "parse_atom: expected ']' after slice end expression",
                                )?;
                                let span = expr.span;
                                expr = Expression::at(
                                    ExpressionKind::Slice {
                                        object: Box::new(expr),
                                        start: Some(Box::new(first)),
                                        end: Some(Box::new(end)),
                                        inclusive: false,
                                    },
                                    span,
                                );
                            }
                        } else if is_dot(&self.peek()) && is_assign(&self.peek_second()) {
                            // `[a.=b]` — inclusive end.
                            self.next(); // consume '.'
                            self.next(); // consume '='
                            if is_close(&self.peek()) {
                                let t = self.peek().expect("peeked ']'");
                                return Err(self.error(
                                    "parse_atom: '.=' needs an end bound, e.g. 'arr.[a.=b]'",
                                    t.line,
                                    t.column,
                                ));
                            }
                            let end = self.parse_slice_bound()?;
                            self.expect_token(
                                TokenKind::Punctuation(Punctuation::ClosingSquareBrace),
                                "parse_atom: expected ']' after slice end expression",
                            )?;
                            let span = expr.span;
                            expr = Expression::at(
                                ExpressionKind::Slice {
                                    object: Box::new(expr),
                                    start: Some(Box::new(first)),
                                    end: Some(Box::new(end)),
                                    inclusive: true,
                                },
                                span,
                            );
                        } else {
                            self.expect_token(
                                TokenKind::Punctuation(Punctuation::ClosingSquareBrace),
                                "parse_atom: expected ']' after index expression",
                            )?;
                            let span = expr.span;
                            expr = Expression::at(
                                ExpressionKind::IndexAccess {
                                    object: Box::new(expr),
                                    index: Box::new(first),
                                },
                                span,
                            );
                        }
                    }
                } else {
                    let next_token =
                        self.expect_next("parse_atom: expected field name or operator after '.'")?;
                    match next_token.kind {
                        TokenKind::Operator(Operator::Star) => {
                            let span = expr.span;
                            expr =
                                Expression::at(ExpressionKind::Dereference(Box::new(expr)), span);
                        }
                        TokenKind::Operator(Operator::Ampersand) => {
                            let span = expr.span;
                            expr = Expression::at(ExpressionKind::AddressOf(Box::new(expr)), span);
                        }
                        // `arr.@len` — a comptime property. Reuse FieldAccess,
                        // spelling the property (with its `@`) as the field so
                        // the analyzer can recognize it before struct lookup.
                        TokenKind::Builtin(Builtin::BuiltinProperty(prop)) => {
                            let span = expr.span;
                            expr = Expression::at(
                                ExpressionKind::FieldAccess {
                                    object: Box::new(expr),
                                    field: crate::frontend::identifier::Identifier {
                                        value: format!("{}", prop),
                                    },
                                },
                                span,
                            );
                        }
                        TokenKind::Identifier(f) => {
                            // If this is `TypeName.Variant { ... }` — an enum
                            // variant construction with payload — capture it
                            // here. Otherwise it's a plain field access.
                            if !self.no_struct_literal
                                && let ExpressionKind::Identifier(type_name) = &expr.kind
                                && matches!(
                                    self.peek(),
                                    Some(Token {
                                        kind: TokenKind::Punctuation(
                                            Punctuation::OpeningCurlyBrace
                                        ),
                                        ..
                                    })
                                )
                            {
                                self.next(); // consume '{'
                                let fields = self.parse_brace_fields("variant payload")?;
                                let tn = type_name.clone();
                                let span = expr.span;
                                expr = Expression::at(
                                    ExpressionKind::EnumVariantConstruct {
                                        type_name: tn,
                                        variant: f,
                                        fields,
                                    },
                                    span,
                                );
                            } else {
                                let span = expr.span;
                                expr = Expression::at(
                                    ExpressionKind::FieldAccess {
                                        object: Box::new(expr),
                                        field: f,
                                    },
                                    span,
                                );
                            }
                        }
                        _ => {
                            return Err(self.error(
                                "expected field name, '.*', '.&', or '.[' after '.'",
                                next_token.line,
                                next_token.column,
                            ));
                        }
                    }
                }
            } else {
                break;
            }
        }

        Ok(expr)
    }

    /// Parse one slice bound (start or end) inside `.[ ... ]`: a full
    /// expression with struct literals re-enabled, but with `.=` reserved
    /// as the inclusive range separator (see `slice_bound`). A bare `.=`
    /// is never a valid expression operator, so nothing valid is lost.
    fn parse_slice_bound(&mut self) -> ParseResult<Expression> {
        let saved_struct = std::mem::replace(&mut self.no_struct_literal, false);
        let saved_slice = std::mem::replace(&mut self.slice_bound, true);
        let result = self.parse_expression();
        self.no_struct_literal = saved_struct;
        self.slice_bound = saved_slice;
        result
    }
}
