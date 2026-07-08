use crate::frontend::{
    ast::{enum_variant::EnumVariant, type_expression::TypeExpression},
    parser::ParseResult,
    tokens::{Punctuation, Token, TokenKind},
};

use super::Parser;
impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    pub fn parse_enum_literal(&mut self, type_token: &Token) -> ParseResult<TypeExpression> {
        // 'enum' already consumed.
        let first = self.expect_token(
            TokenKind::Punctuation(Punctuation::OpeningCurlyBrace),
            "expected '{' after 'enum'",
        )?;
        let _ = first;
        let mut variants: Vec<EnumVariant> = Vec::new();
        while let Some(t) = self.peek() {
            if matches!(
                t.kind,
                TokenKind::Punctuation(Punctuation::ClosingCurlyBrace)
            ) {
                self.next();
                break;
            }
            let name = self.expect_identifier("expected variant name")?;
            // Optional payload: `Variant { field-list }`
            let payload = if matches!(
                self.peek(),
                Some(Token {
                    kind: TokenKind::Punctuation(Punctuation::OpeningCurlyBrace),
                    ..
                })
            ) {
                self.next(); // consume '{'
                let fields = self.parse_type_fields()?;
                Some(fields)
            } else {
                None
            };
            variants.push(EnumVariant { name, payload });
            // optional comma
            self.consume_if(|t| matches!(t.kind, TokenKind::Punctuation(Punctuation::Comma)));
        }
        let _ = type_token;
        Ok(TypeExpression::Enum { variants })
    }
}
