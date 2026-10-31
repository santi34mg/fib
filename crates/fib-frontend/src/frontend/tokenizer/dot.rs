use crate::frontend::{
    tokenizer::Tokenizer,
    tokens::{Operator, Punctuation, TokenKind},
};

impl<'input> Tokenizer<'input> {
    /// Lex `.`: `...`, `..`, or `.=`.
    pub fn lex_dot(&mut self) -> Option<TokenKind> {
        self.bump(); // consume first dot
        if let Some(c) = self.peek() {
            match c {
                '.' => {
                    self.bump(); // consume second dot
                    if self.peek() == Some('.') {
                        self.bump(); // third dot, ellipsis
                        Some(TokenKind::Operator(Operator::Ellipsis))
                    } else {
                        // no third dot, was double dot
                        Some(TokenKind::Operator(Operator::DoubleDot))
                    }
                }
                '=' => {
                    self.bump();
                    Some(TokenKind::Operator(Operator::DotEq))
                }
                _ => Some(TokenKind::Punctuation(Punctuation::Dot)),
            }
        } else {
            Some(TokenKind::Punctuation(Punctuation::Dot))
        }
    }
}
