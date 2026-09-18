use crate::frontend::{
    ast::{expression::Expression, type_expression::TypeExpression},
    identifier::Identifier,
    parser::ParseResult,
    tokens::{Punctuation, Token, TokenKind},
};

use super::Parser;
impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    pub fn parse_struct_literal(&mut self, type_token: &Token) -> ParseResult<TypeExpression> {
        // consume 'struct' keyword
        match self.next() {
            Some(first_token) => match first_token.kind {
                TokenKind::Punctuation(Punctuation::OpeningCurlyBrace) => {
                    let fields = self.parse_type_fields()?;
                    Ok(TypeExpression::Struct { fields })
                }
                _ => Err(self.error(
                    "expected an open curly brace",
                    first_token.line,
                    first_token.column,
                )),
            },
            None => Err(self.error(
                "expected a type keyword",
                type_token.line,
                type_token.column,
            )),
        }
    }

    /// Parse `{ name: expr, ... }` field lists shared by struct construction
    /// (`Type { ... }`) and enum variant payloads (`Type.Variant { ... }`).
    /// Assumes `{` was already consumed; consumes the closing `}`.
    ///
    /// `context` names the construct for errors (`"struct construction"`,
    /// `"variant payload"`). Like the original loops, a missing `}` at EOF
    /// is tolerated: the trailing `next()` result is discarded.
    pub fn parse_brace_fields(
        &mut self,
        context: &str,
    ) -> ParseResult<Vec<(Identifier, Expression)>> {
        let mut fields = Vec::new();
        while !matches!(
            self.peek(),
            Some(Token {
                kind: TokenKind::Punctuation(Punctuation::ClosingCurlyBrace),
                ..
            }) | None
        ) {
            let fname = self.expect_identifier(&format!("expected field name in {}", context))?;
            self.expect_token(
                TokenKind::Punctuation(Punctuation::Colon),
                &format!("expected ':' after field name in {}", context),
            )?;
            let val = self.allow_struct_literals(|p| p.parse_expression())?;
            fields.push((fname, val));
            // optional comma
            self.consume_if(|t| matches!(t.kind, TokenKind::Punctuation(Punctuation::Comma)));
        }
        self.next(); // consume '}'
        Ok(fields)
    }

    /// Parse `TypeName { field: value, ... }` after the type name.
    /// Assumes `{` was already consumed.
    pub fn parse_struct_construct(&mut self, type_name: Identifier) -> ParseResult<Expression> {
        let fields = self.parse_brace_fields("struct construction")?;
        Ok(Expression::StructConstruct { type_name, fields })
    }
}
