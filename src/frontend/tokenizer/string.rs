use crate::frontend::{
    tokenizer::Tokenizer,
    tokens::{Literal, TokenKind},
};

impl<'input> Tokenizer<'input> {
    /// Lex `"..."` string literals with escape sequences.
    pub fn lex_string_literal(&mut self) -> Option<TokenKind> {
        self.bump();
        let mut escaped = String::new();
        let mut lex_error: Option<&str> = None;
        loop {
            match self.peek() {
                None => {
                    lex_error = Some("unterminated string literal");
                    break;
                }
                Some('"') => {
                    self.bump();
                    break;
                }
                Some('\\') => {
                    self.bump();
                    match self.bump() {
                        None => {
                            lex_error = Some("unterminated string escape");
                            break;
                        }
                        Some('n') => escaped.push('\n'),
                        Some('t') => escaped.push('\t'),
                        Some('\\') => escaped.push('\\'),
                        Some('"') => escaped.push('"'),
                        Some('\'') => escaped.push('\''),
                        Some('0') => escaped.push('\0'),
                        Some('r') => escaped.push('\r'),
                        Some(c) => {
                            escaped.push('\\');
                            escaped.push(c);
                        }
                    }
                }
                Some(c) => {
                    self.bump();
                    escaped.push(c);
                }
            }
        }
        if let Some(e) = lex_error {
            Some(TokenKind::Error(e.into()))
        } else {
            Some(TokenKind::Literal(Literal::String(escaped)))
        }
    }
}
