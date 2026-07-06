use crate::frontend::{
    ast::type_expression::TypeExpression,
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
}
