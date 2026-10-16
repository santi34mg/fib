use crate::frontend::{
    next_ast::type_declaration::TypeDeclaration,
    tokens::{Keyword, Token, TokenKind},
};

use super::Parser;

impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    pub fn parse_type_declaration(&mut self) -> anyhow::Result<TypeDeclaration> {
        let t = self.expect_token(TokenKind::Keyword(Keyword::Type), "expected keyword 'type'")?;

        let identifier = self.expect_unqualified_name("expected identifier")?;

        let expression = self.parse_type_expression()?.ok_or(self.error(
            "expected type expression",
            t.line,
            t.column,
        ))?;

        Ok(TypeDeclaration {
            identifier,
            expression,
        })
    }
}
