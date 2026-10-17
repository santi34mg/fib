use crate::frontend::{
    ast::type_declaration::TypeDeclaration,
    tokens::{
        Token, TokenKind,
        builtin::{Builtin, BuiltinType},
    },
};

use super::Parser;

impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    pub fn parse_type_declaration(&mut self) -> anyhow::Result<TypeDeclaration> {
        let t = self.expect_token(
            TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::MetaType)),
            "expected 'type'",
        )?;

        let identifier = self.expect_identifier("expected identifier")?;

        let expression = self.parse_type_expression()?.ok_or(self.error(
            "expected type expression",
            t.line,
            t.column,
        ))?;

        Ok(TypeDeclaration {
            identifier,
            annotation: Default::default(),
            expression,
        })
    }
}
