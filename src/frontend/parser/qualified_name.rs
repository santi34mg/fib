use crate::frontend::{
    ast::qualified_name::QualifiedIdentifier,
    tokens::{Punctuation, Token, TokenKind},
};

use super::Parser;
impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    pub fn parse_qualified_name(&mut self) -> anyhow::Result<QualifiedIdentifier> {
        let base = self.expect_identifier("expected an identifier")?;
        // Check for `::` — qualified type: module::TypeName
        let mut symbols = vec![base];
        while let Some(Token {
            kind: TokenKind::Punctuation(Punctuation::DoubleColon),
            ..
        }) = self.peek()
        {
            self.next(); // consume `::`
            let name = self.expect_identifier("'::' must be followed by symbol")?;
            symbols.push(name);
        }
        Ok(QualifiedIdentifier::new(symbols))
    }
}
