use crate::frontend::{
    ast::expression::{Expression, ExpressionKind},
    parser::ParseResult,
    tokens::{Operator, Punctuation, Token, TokenKind},
};

use super::Parser;

impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    /// Parse postfix operations after a primary expression: function calls
    /// (`f(args)`), field access (`.field`), dereference (`.*`),
    /// address-of (`.&`), index access (`.[i]`), and enum variant
    /// construction (`Type.Variant { ... }`).
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
                    let index = self.allow_struct_literals(|p| p.parse_expression())?;
                    self.expect_token(
                        TokenKind::Punctuation(Punctuation::ClosingSquareBrace),
                        "parse_atom: expected ']' after index expression",
                    )?;
                    let span = expr.span;
                    expr = Expression::at(
                        ExpressionKind::IndexAccess {
                            object: Box::new(expr),
                            index: Box::new(index),
                        },
                        span,
                    );
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
}
