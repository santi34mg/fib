use crate::frontend::{
    ast::{expression::Expression, type_expression::TypeExpression},
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
        match token.kind {
            TokenKind::Literal(Literal::Integer(integer_literal)) => {
                Ok(Expression::Literal(Literal::Integer(integer_literal)))
            }
            TokenKind::Literal(Literal::Float(float_literal)) => {
                Ok(Expression::Literal(Literal::Float(float_literal)))
            }
            TokenKind::Literal(Literal::Boolean(boolean_literal)) => {
                Ok(Expression::Literal(Literal::Boolean(boolean_literal)))
            }
            TokenKind::Literal(Literal::Character(char_literal)) => {
                Ok(Expression::Literal(Literal::Character(char_literal)))
            }
            TokenKind::Literal(Literal::String(s)) => Ok(Expression::Literal(Literal::String(s))),
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
                    Ok(Expression::QualifiedAccess { module: id, member })
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
                    self.parse_struct_construct(id)
                } else {
                    Ok(Expression::Identifier(id))
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
                Ok(Expression::ArrayLiteral { elements })
            }
            TokenKind::Punctuation(Punctuation::OpeningParenthesis) => {
                let inner_expr = self.allow_struct_literals(|p| p.parse_expression())?;
                self.expect_token(
                    TokenKind::Punctuation(Punctuation::ClosingParenthesis),
                    "parse_atom: expected ')'",
                )?;
                Ok(Expression::Grouping(Box::new(inner_expr)))
            }
            // A builtin type token in expression position — produces a comptime type value.
            // This allows passing builtin types as generic arguments: `identity(sint32, 42)`
            TokenKind::Builtin(Builtin::BuiltinType(bt)) => {
                Ok(Expression::TypeValue(TypeExpression::Builtin(bt)))
            }
            // A builtin function call, e.g. `@concat(a, b)`. The parentheses are
            // mandatory — a bare `@concat` is an error.
            TokenKind::Builtin(Builtin::BuiltinFunction(bf)) => {
                self.expect_token(
                    TokenKind::Punctuation(Punctuation::OpeningParenthesis),
                    "parse_atom: expected '(' after builtin function",
                )?;
                let args = self.parse_call_arguments("builtin function arguments")?;
                Ok(Expression::BuiltinCall { builtin: bf, args })
            }
            TokenKind::Literal(Literal::Null) => Ok(Expression::Literal(Literal::Null)),
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
