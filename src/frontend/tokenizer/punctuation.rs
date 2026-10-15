use crate::frontend::{
    tokenizer::Tokenizer,
    tokens::{Punctuation, TokenKind},
};

impl<'input> Tokenizer<'input> {
    /// Lex punctuation, including the two-character `::`.
    pub fn lex_punctuation(&mut self, c: char) -> Option<TokenKind> {
        match c {
            '(' => {
                self.bump();
                Some(TokenKind::Punctuation(Punctuation::OpeningParenthesis))
            }
            ')' => {
                self.bump();
                Some(TokenKind::Punctuation(Punctuation::ClosingParenthesis))
            }
            '{' => {
                self.bump();
                Some(TokenKind::Punctuation(Punctuation::OpeningCurlyBrace))
            }
            '}' => {
                self.bump();
                Some(TokenKind::Punctuation(Punctuation::ClosingCurlyBrace))
            }
            '[' => {
                self.bump();
                Some(TokenKind::Punctuation(Punctuation::OpeningSquareBrace))
            }
            ']' => {
                self.bump();
                Some(TokenKind::Punctuation(Punctuation::ClosingSquareBrace))
            }
            ',' => {
                self.bump();
                Some(TokenKind::Punctuation(Punctuation::Comma))
            }
            ';' => {
                self.bump();
                Some(TokenKind::Punctuation(Punctuation::Semicolon))
            }
            ':' => {
                self.bump();
                if self.peek() == Some(':') {
                    self.bump();
                    Some(TokenKind::Punctuation(Punctuation::DoubleColon))
                } else {
                    Some(TokenKind::Punctuation(Punctuation::Colon))
                }
            }
            '@' => {
                self.bump();
                Some(TokenKind::Punctuation(Punctuation::At))
            }
            _ => None,
        }
    }
}
