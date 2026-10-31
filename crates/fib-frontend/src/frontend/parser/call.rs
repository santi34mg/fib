use crate::frontend::{ast::expression::Expression, tokens::Token};

use super::Parser;

impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    /// Parse `(arg, ...)` argument lists for ordinary postfix calls.
    /// Assumes `(` was already consumed; consumes the closing `)`.
    /// `close_what` completes the error message (`"function
    /// arguments"`, `"function call arguments"`).
    pub fn parse_call_arguments(&mut self, close_what: &str) -> anyhow::Result<Vec<Expression>> {
        use crate::frontend::tokens::{Punctuation, TokenKind};

        let mut args = Vec::new();
        if let Some(token) = self.peek()
            && !matches!(
                token.kind,
                TokenKind::Punctuation(Punctuation::ClosingParenthesis)
            )
        {
            loop {
                args.push(self.allow_struct_literals(|p| p.parse_expression())?);
                if let Some(token) = self.peek()
                    && matches!(token.kind, TokenKind::Punctuation(Punctuation::Comma))
                {
                    self.next(); // consume ','
                } else {
                    break;
                }
            }
        }
        self.expect_token(
            TokenKind::Punctuation(Punctuation::ClosingParenthesis),
            &format!("parse_atom: expected ')' after {}", close_what),
        )?;
        Ok(args)
    }
}
