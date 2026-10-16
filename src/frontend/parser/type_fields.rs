use crate::{
    diagnostics::Span,
    frontend::{
        ast::{
            expression::{Expression, ExpressionKind},
            structl_field::StructField,
        },
        tokens::{Punctuation, Token, TokenKind},
    },
};

use super::Parser;
impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    pub fn parse_type_fields(&mut self) -> anyhow::Result<Vec<StructField>> {
        let mut fields: Vec<StructField> = Vec::new();
        while let Some(next_token) = self.peek() {
            if let TokenKind::Punctuation(Punctuation::ClosingCurlyBrace) = next_token.kind {
                self.next(); // consume the closing curly brace
                break;
            }
            let name = self.expect_unqualified_name("expected field name")?;
            self.expect_token(
                TokenKind::Punctuation(Punctuation::Colon),
                "expected ':' after field name",
            )?;
            let line = next_token.line;
            let column = next_token.column;
            let field_type = match self.parse_type_expression()? {
                Some(field_type) => field_type,
                None => {
                    return Err(self.error("expected a type identifier", line, column));
                }
            };
            fields.push(StructField {
                field_name: name,
                expression: Expression::at(
                    ExpressionKind::TypeValue(field_type),
                    Span::new(line, column),
                ),
            });
            self.consume_if(|t| matches!(t.kind, TokenKind::Punctuation(Punctuation::Comma)));
        }
        Ok(fields)
    }
}
