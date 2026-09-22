use crate::diagnostics::Span;
use crate::frontend::{
    ast::type_expression::{TypeExpression, TypeExpressionKind},
    parser::ParseResult,
    tokens::{Keyword, Literal, Operator, Punctuation, Token, TokenKind, builtin::Builtin},
};

use super::Parser;
impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    /// Parses a type annotation, which can be a user-defined type (type identifier), or complex type (struct, variant or function).
    /// Returns `Ok(Some(TypeIdentifier))` if a type is successfully parsed, `Ok(None)` if the next token is not a type, or `Err` if there's a syntax error while parsing a type.
    pub fn parse_type_expression(&mut self) -> ParseResult<Option<TypeExpression>> {
        let type_token = if let Some(t) = self.peek() {
            t
        } else {
            return Ok(None);
        };
        let span = Span::new(type_token.line, type_token.column);
        let var_type: TypeExpression = match type_token.kind {
            TokenKind::Builtin(Builtin::BuiltinType(builtin_type)) => {
                self.next();
                TypeExpression::at(TypeExpressionKind::Builtin(builtin_type), span)
            }
            TokenKind::Keyword(ref keyword) => {
                self.next();
                match keyword {
                    Keyword::Struct => self.parse_struct_literal(&type_token)?,
                    Keyword::Enum => self.parse_enum_literal(&type_token)?,
                    Keyword::Function => self.parse_function_type(&type_token)?,
                    Keyword::Type => TypeExpression::at(TypeExpressionKind::TypeKeyword, span),
                    _ => return Err(self.error("not a type", type_token.line, type_token.column)),
                }
            }
            TokenKind::Operator(Operator::Star) => {
                self.next();
                self.parse_pointer_type(type_token)?
            }
            TokenKind::Punctuation(Punctuation::OpeningParenthesis) => {
                self.next(); // consume '('
                self.parse_tuple_type_expression(&type_token)?
            }
            TokenKind::Identifier(module) => {
                self.next();
                // Check for `::` — qualified type: module::TypeName
                if matches!(
                    self.peek(),
                    Some(Token {
                        kind: TokenKind::Punctuation(Punctuation::DoubleColon),
                        ..
                    })
                ) {
                    self.next(); // consume `::`
                    let name = self.expect_identifier("expected type name after '::'")?;
                    TypeExpression::at(
                        TypeExpressionKind::QualifiedIdentifier { module, name },
                        span,
                    )
                } else {
                    TypeExpression::at(TypeExpressionKind::Identifier(module), span)
                }
            }
            _ => {
                // Surface lexer diagnostics instead of silently reporting
                // "not a type" downstream; anything else is not a type.
                match &type_token.kind {
                    TokenKind::Error(msg) => {
                        return Err(self.error(msg, type_token.line, type_token.column));
                    }
                    TokenKind::Unknown(c) => {
                        return Err(self.error(
                            &format!("unknown character '{}'", c),
                            type_token.line,
                            type_token.column,
                        ));
                    }
                    _ => {
                        return Ok(None);
                    }
                }
            }
        };
        // Postfix array/slice type: `type[size]` or `type[]`.
        let var_type = if matches!(
            self.peek(),
            Some(Token {
                kind: TokenKind::Punctuation(Punctuation::OpeningSquareBrace),
                ..
            })
        ) {
            self.next(); // consume '['
            // `T[]` — a slice (runtime-length view).
            if matches!(
                self.peek(),
                Some(Token {
                    kind: TokenKind::Punctuation(Punctuation::ClosingSquareBrace),
                    ..
                })
            ) {
                self.next(); // consume ']'
                TypeExpression::at(
                    TypeExpressionKind::Slice {
                        element_type: Box::new(var_type),
                    },
                    span,
                )
            } else {
                let size_token = self.expect_next("expected array size")?;
                let size = if let TokenKind::Literal(Literal::Integer(n)) = size_token.kind {
                    n
                } else {
                    return Err(self.error(
                        "expected integer array size",
                        size_token.line,
                        size_token.column,
                    ));
                };
                self.expect_token(
                    TokenKind::Punctuation(Punctuation::ClosingSquareBrace),
                    "expected ']' after array size",
                )?;
                TypeExpression::at(
                    TypeExpressionKind::Array {
                        element_type: Box::new(var_type),
                        size,
                    },
                    span,
                )
            }
        } else {
            var_type
        };
        Ok(Some(var_type))
    }
}
