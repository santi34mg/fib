use crate::frontend::{
    ast::type_expression::TypeExpression,
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
        let var_type: TypeExpression = match type_token.kind {
            TokenKind::Builtin(Builtin::BuiltinType(builtin_type)) => {
                self.next();
                TypeExpression::Builtin(builtin_type)
            }
            TokenKind::Keyword(ref keyword) => {
                self.next();
                match keyword {
                    Keyword::Struct => self.parse_struct_literal(&type_token)?,
                    Keyword::Enum => self.parse_enum_literal(&type_token)?,
                    Keyword::Function => self.parse_function_type(&type_token)?,
                    Keyword::Type => TypeExpression::TypeKeyword,
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
                    TypeExpression::QualifiedIdentifier { module, name }
                } else {
                    TypeExpression::Identifier(module)
                }
            }
            _ => {
                return Ok(None);
            }
        };
        // Postfix array type: type[size]
        let var_type = if matches!(
            self.peek(),
            Some(Token {
                kind: TokenKind::Punctuation(Punctuation::OpeningSquareBrace),
                ..
            })
        ) {
            self.next(); // consume '['
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
            TypeExpression::Array {
                element_type: Box::new(var_type),
                size,
            }
        } else {
            var_type
        };
        Ok(Some(var_type))
    }
}
