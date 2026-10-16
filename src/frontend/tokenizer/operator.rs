use crate::frontend::{
    tokenizer::Tokenizer,
    tokens::{Operator, TokenKind},
};

impl<'input> Tokenizer<'input> {
    /// Lex symbolic operators (`==`, `!=`, `<<`, `>>`, `&&`, `||`, `+=`,
    /// `->`, ...). Only called for characters in the operator class.
    pub fn lex_operator(&mut self, c: char) -> Option<TokenKind> {
        match c {
            '=' => {
                self.bump();
                if self.peek() == Some('=') {
                    self.bump();
                    Some(TokenKind::Operator(Operator::DoubleEquals))
                } else {
                    Some(TokenKind::Operator(Operator::Assign))
                }
            }
            '!' => {
                self.bump();
                if self.peek() == Some('=') {
                    self.bump();
                    Some(TokenKind::Operator(Operator::Different))
                } else {
                    Some(TokenKind::Operator(Operator::LogicalNot))
                }
            }
            '>' => {
                self.bump();
                let c = self.peek();
                match c {
                    Some('>') => {
                        self.bump();
                        Some(TokenKind::Operator(Operator::RightShift))
                    }
                    Some('=') => {
                        self.bump();
                        Some(TokenKind::Operator(Operator::GreaterEqual))
                    }
                    _ => Some(TokenKind::Operator(Operator::GreaterThan)),
                }
            }
            '<' => {
                self.bump();
                let c = self.peek();
                match c {
                    Some('<') => {
                        self.bump();
                        Some(TokenKind::Operator(Operator::LeftShift))
                    }
                    Some('=') => {
                        self.bump();
                        Some(TokenKind::Operator(Operator::LesserEqual))
                    }
                    _ => Some(TokenKind::Operator(Operator::LesserThan)),
                }
            }
            '+' => {
                self.bump();
                let c = self.peek();
                match c {
                    Some('=') => {
                        self.bump();
                        Some(TokenKind::Operator(Operator::PlusAssign))
                    }
                    _ => Some(TokenKind::Operator(Operator::Plus)),
                }
            }
            '-' => {
                self.bump();
                let c = self.peek();
                match c {
                    Some('>') => {
                        self.bump();
                        Some(TokenKind::Operator(Operator::ThinRightArrow))
                    }
                    Some('=') => {
                        self.bump();
                        Some(TokenKind::Operator(Operator::MinusAssign))
                    }
                    _ => Some(TokenKind::Operator(Operator::Minus)),
                }
            }
            '*' => {
                self.bump();
                let c = self.peek();
                match c {
                    Some('=') => {
                        self.bump();
                        Some(TokenKind::Operator(Operator::StarAssign))
                    }
                    _ => Some(TokenKind::Operator(Operator::Star)),
                }
            }
            '%' => {
                self.bump();
                let c = self.peek();
                match c {
                    Some('=') => {
                        self.bump();
                        Some(TokenKind::Operator(Operator::PercentAssign))
                    }
                    _ => Some(TokenKind::Operator(Operator::Percent)),
                }
            }
            '&' => {
                self.bump();
                if self.peek() == Some('&') {
                    self.bump();
                    Some(TokenKind::Operator(Operator::LogicalAnd))
                } else {
                    Some(TokenKind::Operator(Operator::Ampersand))
                }
            }
            '|' => {
                self.bump();
                if self.peek() == Some('|') {
                    self.bump();
                    Some(TokenKind::Operator(Operator::LogicalOr))
                } else {
                    Some(TokenKind::Operator(Operator::Pipe))
                }
            }
            '^' => {
                self.bump();
                Some(TokenKind::Operator(Operator::Caret))
            }
            '~' => {
                self.bump();
                Some(TokenKind::Operator(Operator::Tilde))
            }
            _ => None,
        }
    }
}
