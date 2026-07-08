use crate::frontend::{ast::field::Field, parser::ParseResult, tokens::{Punctuation, Token, TokenKind}};

use super::Parser;
impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    pub fn parse_type_fields(&mut self) -> ParseResult<Vec<Field>> {
        let mut fields: Vec<Field> = Vec::new();
        while let Some(next_token) = self.peek() {
            if let TokenKind::Punctuation(Punctuation::ClosingCurlyBrace) = next_token.kind {
                self.next(); // consume the closing curly brace
                break;
            }
            let label = self.expect_identifier("expected field name")?;
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
            fields.push(Field {
                label,
                type_id: field_type,
            });
            self.consume_if(|t| matches!(t.kind, TokenKind::Punctuation(Punctuation::Comma)));
        }
        Ok(fields)
    }
}
