use crate::frontend::{
    tokenizer::Tokenizer,
    tokens::{Literal, TokenKind},
};

impl<'input> Tokenizer<'input> {
    pub fn lex_char_literal(&mut self) -> Option<TokenKind> {
        // consume opening quote
        self.bump();
        // get the character
        let Some(ch) = self.bump() else {
            return Some(TokenKind::Error("unterminated character literal".into()));
        };
        // A bare quote cannot be the character body: a literal quote must be
        // written as an escape (`'\''`). Reject an unescaped quote so `'''` is
        // not silently accepted as the character `'`.
        if ch == '\'' {
            return Some(TokenKind::Error(
                "unescaped quote in character literal".into(),
            ));
        }
        // if char is '\\' then it is an escape sequence
        let ch = if ch == '\\' {
            let Some(esc) = self.bump() else {
                return Some(TokenKind::Error("unterminated character escape".into()));
            };
            match esc {
                'n' => '\n',
                't' => '\t',
                '\\' => '\\',
                '\'' => '\'',
                '\"' => '\"',
                '0' => '\0',
                'x' => {
                    let mut hex = String::new();
                    // get two hex digits
                    for _ in 0..2 {
                        let Some(c) = self.bump() else {
                            return Some(TokenKind::Error("unterminated hex escape".into()));
                        };
                        if c.is_ascii_hexdigit() {
                            hex.push(c);
                        } else {
                            return Some(TokenKind::Error("Invalid hex escape sequence".into()));
                        }
                    }
                    match u8::from_str_radix(&hex, 16) {
                        Ok(value) => value as char,
                        Err(_) => {
                            return Some(TokenKind::Error(format!(
                                "Invalid hex escape: \\x{}",
                                hex
                            )));
                        }
                    }
                }
                'u' => {
                    if self.peek() == Some('{') {
                        self.bump(); // consume '{'
                        let mut hex = String::new();
                        while let Some(c) = self.peek() {
                            if c == '}' {
                                break;
                            }
                            if c.is_ascii_hexdigit() {
                                hex.push(c);
                                self.bump();
                            } else {
                                return Some(TokenKind::Error(
                                    "Invalid unicode escape sequence".into(),
                                ));
                            }
                        }
                        self.bump(); // consume '}'
                        let value = match u32::from_str_radix(&hex, 16) {
                            Ok(v) => v,
                            Err(_) => {
                                return Some(TokenKind::Error(format!(
                                    "Invalid unicode escape: \\u{{{}}}",
                                    hex
                                )));
                            }
                        };
                        match char::from_u32(value) {
                            Some(c) => c,
                            None => {
                                return Some(TokenKind::Error(format!(
                                    "Invalid unicode code point: {}",
                                    value
                                )));
                            }
                        }
                    } else {
                        return Some(TokenKind::Error(
                            "Invalid unicode escape sequence: expected '{'".into(),
                        ));
                    }
                }
                c => {
                    return Some(TokenKind::Error(format!(
                        "Invalid escape sequence: \\{}",
                        c
                    )));
                }
            }
        } else {
            ch
        };
        match self.bump() {
            Some('\'') => Some(TokenKind::Literal(Literal::Character(ch))),
            _ => Some(TokenKind::Error(
                "character literal missing closing quote".into(),
            )),
        }
    }
}
