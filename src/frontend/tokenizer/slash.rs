use crate::frontend::{
    tokenizer::Tokenizer,
    tokens::{Operator, TokenKind},
};

impl<'input> Tokenizer<'input> {
    /// Lex `/`: line comments (`//...`), `/=`, or `/`.
    pub fn lex_slash(&mut self) -> Option<TokenKind> {
        self.bump();
        let c = self.peek();
        match c {
            Some('/') => {
                self.bump();
                self.skip_while(|c| c != '\n');
                Some(TokenKind::Comment)
            }
            Some('=') => {
                self.bump();
                Some(TokenKind::Operator(Operator::SlashAssign))
            }
            _ => Some(TokenKind::Operator(Operator::Slash)),
        }
    }
}
