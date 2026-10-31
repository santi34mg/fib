use crate::diagnostics::Span;
use crate::frontend::{
    ast::{
        enum_variant::EnumVariant,
        type_expression::{TypeExpression, TypeExpressionKind},
    },
    tokens::{Punctuation, Token, TokenKind},
};

use super::Parser;
impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    pub fn parse_enum_literal(&mut self, type_token: &Token) -> anyhow::Result<TypeExpression> {
        // 'enum' already consumed.
        self.expect_token(
            TokenKind::Punctuation(Punctuation::OpeningCurlyBrace),
            "expected '{' after 'enum'",
        )?;
        let mut variants: Vec<EnumVariant> = Vec::new();
        loop {
            let t = self.peek().ok_or_else(|| {
                self.error(
                    "expected '}' after enum variants",
                    type_token.line,
                    type_token.column,
                )
            })?;
            if matches!(
                t.kind,
                TokenKind::Punctuation(Punctuation::ClosingCurlyBrace)
            ) {
                self.next();
                break;
            }
            let name = self.expect_identifier("expected variant name")?;
            self.mark_name("enumMember");
            if matches!(
                self.peek(),
                Some(Token {
                    kind: TokenKind::Punctuation(Punctuation::OpeningCurlyBrace),
                    ..
                })
            ) {
                return Err(self.error("enum payload fields require an explicit type; use 'Variant: struct { field: Type }'", t.line, t.column));
            }
            let payload = if self
                .consume_if(|t| matches!(t.kind, TokenKind::Punctuation(Punctuation::Colon)))
                .is_some()
            {
                Some(Box::new(
                    self.allow_struct_literals(|p| p.parse_expression())?,
                ))
            } else {
                None
            };
            variants.push(EnumVariant {
                identifier: name,
                payload,
            });
            // optional comma
            self.consume_if(|t| matches!(t.kind, TokenKind::Punctuation(Punctuation::Comma)));
        }
        Ok(TypeExpression::at(
            TypeExpressionKind::Enum { variants },
            Span::new(type_token.line, type_token.column),
        ))
    }
}
