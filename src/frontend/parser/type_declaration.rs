use crate::frontend::{ast::type_declaration::TypeDeclaration, parser::ParseResult, tokens::{Keyword, Token, TokenKind}};

use super::Parser;

impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    pub fn parse_type_declaration(&mut self) -> ParseResult<TypeDeclaration> {
        let t = self.expect_token(TokenKind::Keyword(Keyword::Type), "expected keyword 'type'")?;

        let name = self.expect_identifier("expected identifier")?;

        let expression = self.parse_type_expression()?.ok_or(self.error(
            "expected type expression",
            t.line,
            t.column,
        ))?;

        Ok(TypeDeclaration { name, expression })
    }
}
