use crate::diagnostics::Span;
use crate::frontend::{
    ast::{
        expression::{Expression, ExpressionKind},
        type_expression::{TypeExpression, TypeExpressionKind},
    },
    parser::ParseResult,
    tokens::{Literal, Punctuation, Token, TokenKind, builtin::Builtin},
};

use super::Parser;

impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    /// Parse a primary expression: literals, identifiers (including
    /// `module::member` and `Type { ... }`), array literals, grouping,
    /// builtin type values, builtin calls, and `null`.
    /// Postfix operators (calls, field access, ...) are handled by
    /// [`Parser::parse_postfix`].
    pub fn parse_primary(&mut self) -> ParseResult<Expression> {
        let token = self.expect_next("parse_atom: expected a token, found none")?;
        let span = Span::new(token.line, token.column);
        match token.kind {
            TokenKind::Literal(Literal::Integer(integer_literal)) => Ok(Expression::at(
                ExpressionKind::Literal(Literal::Integer(integer_literal)),
                span,
            )),
            TokenKind::Literal(Literal::Float(float_literal)) => Ok(Expression::at(
                ExpressionKind::Literal(Literal::Float(float_literal)),
                span,
            )),
            TokenKind::Literal(Literal::Boolean(boolean_literal)) => Ok(Expression::at(
                ExpressionKind::Literal(Literal::Boolean(boolean_literal)),
                span,
            )),
            TokenKind::Literal(Literal::Character(char_literal)) => Ok(Expression::at(
                ExpressionKind::Literal(Literal::Character(char_literal)),
                span,
            )),
            TokenKind::Literal(Literal::String(s)) => Ok(Expression::at(
                ExpressionKind::Literal(Literal::String(s)),
                span,
            )),
            TokenKind::Identifier(id) => {
                // Check if next token is `::` — qualified access: module::member
                if matches!(
                    self.peek(),
                    Some(Token {
                        kind: TokenKind::Punctuation(Punctuation::DoubleColon),
                        ..
                    })
                ) {
                    self.next(); // consume `::`
                    let member = self.expect_identifier("expected member name after '::'")?;
                    Ok(Expression::at(
                        ExpressionKind::QualifiedAccess { module: id, member },
                        span,
                    ))
                // Check if next token is '{' — struct construction: TypeName { field: val, ... }
                } else if !self.no_struct_literal
                    && matches!(
                        self.peek(),
                        Some(Token {
                            kind: TokenKind::Punctuation(Punctuation::OpeningCurlyBrace),
                            ..
                        })
                    )
                {
                    self.next(); // consume '{'
                    self.parse_struct_construct(id, span)
                } else {
                    Ok(Expression::at(ExpressionKind::Identifier(id), span))
                }
            }
            TokenKind::Punctuation(Punctuation::OpeningSquareBrace) => {
                let mut elements = Vec::new();
                while !matches!(
                    self.peek(),
                    Some(Token {
                        kind: TokenKind::Punctuation(Punctuation::ClosingSquareBrace),
                        ..
                    }) | None
                ) {
                    elements.push(self.allow_struct_literals(|p| p.parse_expression())?);
                    self.consume_if(|t| {
                        matches!(t.kind, TokenKind::Punctuation(Punctuation::Comma))
                    });
                }
                self.expect_token(
                    TokenKind::Punctuation(Punctuation::ClosingSquareBrace),
                    "parse_atom: expected ']' after array literal",
                )?;
                Ok(Expression::at(
                    ExpressionKind::ArrayLiteral { elements },
                    span,
                ))
            }
            TokenKind::Punctuation(Punctuation::OpeningParenthesis) => {
                let inner_expr = self.allow_struct_literals(|p| p.parse_expression())?;
                self.expect_token(
                    TokenKind::Punctuation(Punctuation::ClosingParenthesis),
                    "parse_atom: expected ')'",
                )?;
                Ok(Expression::at(
                    ExpressionKind::Grouping(Box::new(inner_expr)),
                    span,
                ))
            }
            // A builtin type token in expression position — produces a comptime type value.
            // This allows passing builtin types as generic arguments: `identity(sint32, 42)`
            TokenKind::Builtin(Builtin::BuiltinType(bt)) => Ok(Expression::at(
                ExpressionKind::TypeValue(TypeExpression::at(
                    TypeExpressionKind::Builtin(bt),
                    span,
                )),
                span,
            )),
            // A builtin function call, e.g. `@concat(a, b)`. The parentheses are
            // mandatory — a bare `@concat` is an error.
            TokenKind::Builtin(Builtin::BuiltinFunction(bf)) => {
                self.expect_token(
                    TokenKind::Punctuation(Punctuation::OpeningParenthesis),
                    "parse_atom: expected '(' after builtin function",
                )?;
                let args = self.parse_call_arguments("builtin function arguments")?;
                Ok(Expression::at(
                    ExpressionKind::BuiltinCall { builtin: bf, args },
                    span,
                ))
            }
            TokenKind::Literal(Literal::Null) => {
                Ok(Expression::at(ExpressionKind::Literal(Literal::Null), span))
            }
            // Surface lexer diagnostics verbatim (e.g. unterminated
            // strings, unknown `@builtins`) instead of a generic
            // "expected an atom" message.
            TokenKind::Error(msg) => Err(self.error(&msg, token.line, token.column)),
            TokenKind::Unknown(c) => Err(self.error(
                &format!("unknown character '{}'", c),
                token.line,
                token.column,
            )),
            _ => Err(self.error(
                &format!("parse_atom: expected an atom, found {:?}", token.kind),
                token.line,
                token.column,
            )),
        }
    }
}
