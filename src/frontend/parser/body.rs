use crate::frontend::{
    ast::statement::Statement,
    parser::ParseResult,
    tokens::{Punctuation, Token, TokenKind},
};

use super::Parser;

impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    /// Parse a block body: expects '{' then parses statements until matching '}'.
    pub fn parse_body(&mut self) -> ParseResult<Vec<Statement>> {
        self.expect_token(
            TokenKind::Punctuation(Punctuation::OpeningCurlyBrace),
            "parse_body: expected '{'",
        )?;

        let mut stmts = Vec::new();
        loop {
            match self.peek() {
                None => {
                    let (line, column) = self.last_pos;
                    return Err(self.error("unclosed block: expected '}'", line, column));
                }
                Some(token)
                    if matches!(
                        token.kind,
                        TokenKind::Punctuation(Punctuation::ClosingCurlyBrace)
                    ) =>
                {
                    self.next();
                    break;
                }
                _ => {}
            }
            if let Some(stmt) = self.parse_statement()? {
                stmts.push(stmt);
            }
        }
        Ok(stmts)
    }
}
