use anyhow::bail;

use crate::frontend::{
    ast::qualified_name::QualifiedName,
    tokens::{Punctuation, Token, TokenKind},
};

use super::Parser;
impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    pub fn parse_qualified_name(&mut self) -> anyhow::Result<QualifiedName> {
        if let Some(Token {
            kind: TokenKind::Name(base),
            ..
        }) = self.next()
        {
            // Check for `::` — qualified type: module::TypeName
            let mut symbols = vec![base];
            while let Some(Token {
                kind: TokenKind::Punctuation(Punctuation::DoubleColon),
                ..
            }) = self.peek()
            {
                self.next(); // consume `::`
                let name = self.expect_unqualified_name("'::' must be followed by symbol")?;
                symbols.push(name);
            }
            Ok(QualifiedName { segments: symbols })
        } else {
            bail!("expected an identifier")
        }
    }
}
