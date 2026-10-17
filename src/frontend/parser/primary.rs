use anyhow::bail;

use crate::diagnostics::Span;
use crate::frontend::ast::qualified_name::QualifiedIdentifier;
use crate::frontend::{
    ast::{
        expression::{Expression, ExpressionKind},
        type_expression::{TypeExpression, TypeExpressionKind},
    },
    tokens::{Punctuation, Token, TokenKind, builtin::Builtin},
};

use super::Parser;

impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    /// In a condition, braces after a name may be either struct fields or the
    /// statement body. Fields begin with `name:` and have no top-level `;`.
    fn brace_starts_struct_fields(&mut self) -> bool {
        if !matches!(
            self.peek_n(2),
            Some(Token {
                kind: TokenKind::Identifier(_),
                ..
            })
        ) || !matches!(
            self.peek_n(3),
            Some(Token {
                kind: TokenKind::Punctuation(Punctuation::Colon),
                ..
            })
        ) {
            return false;
        }

        let mut depth = 0;
        let mut n = 1;
        while let Some(token) = self.peek_n(n) {
            match token.kind {
                TokenKind::Punctuation(Punctuation::OpeningCurlyBrace) => depth += 1,
                TokenKind::Punctuation(Punctuation::ClosingCurlyBrace) => {
                    depth -= 1;
                    if depth == 0 {
                        return true;
                    }
                }
                TokenKind::Punctuation(Punctuation::Semicolon) if depth == 1 => return false,
                _ => {}
            }
            n += 1;
        }
        false
    }

    /// Parse a primary expression: literals, identifiers (including qualified names),
    /// array literals, grouping, builtin type values, builtin calls, and `null`.
    /// Postfix operators (calls, field access, ...) are handled by
    /// [`Parser::parse_postfix`].
    pub fn parse_primary(&mut self) -> anyhow::Result<Expression> {
        let token = self.expect_next("parse_atom: expected a token, found none")?;
        let span = Span::new(token.line, token.column);
        match token.kind {
            TokenKind::Literal(literal) => {
                Ok(Expression::at(ExpressionKind::Literal(literal), span))
            }
            TokenKind::Identifier(name) => {
                // Check if next token is `::` — qualified access: module::member
                let mut segments = vec![name];
                while matches!(
                    self.peek(),
                    Some(Token {
                        kind: TokenKind::Punctuation(Punctuation::DoubleColon),
                        ..
                    })
                ) {
                    self.next(); // consume `::`
                    segments.push(self.expect_identifier("expected name after '::'")?);
                }
                if let Some(Token {
                    kind: TokenKind::Punctuation(Punctuation::OpeningCurlyBrace),
                    ..
                }) = self.peek()
                    && (!self.no_struct_literal || self.brace_starts_struct_fields())
                {
                    self.next(); // consume '{'
                    self.parse_struct_construct(QualifiedIdentifier::new(segments), span) // this one consumes '}', TODO: test that
                } else {
                    Ok(Expression::at(
                        ExpressionKind::QualifiedIdentifier(QualifiedIdentifier::new(segments)),
                        span,
                    ))
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
                Ok(inner_expr)
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
                    ExpressionKind::CallToBuiltin { builtin: bf, args },
                    span,
                ))
            }

            // Surface lexer diagnostics verbatim (e.g. unterminated
            // strings, unknown `@builtins`) instead of a generic
            // "expected an atom" message.
            TokenKind::Error(msg) => Err(self.error(&msg, token.line, token.column)),
            TokenKind::Unknown(c) => bail!("unknown character '{}'", c),
            _ => bail!("parse_atom: expected an atom, found {:?}", token.kind),
        }
    }
}
