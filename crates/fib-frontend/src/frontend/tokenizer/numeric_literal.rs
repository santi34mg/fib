use crate::frontend::{
    tokenizer::Tokenizer,
    tokens::{Literal, TokenKind},
};

impl<'input> Tokenizer<'input> {
    pub fn lex_numeric(&mut self, first: char) -> Option<TokenKind> {
        let mut start = self.position;
        // first char is 0 might be 0x... or might be 0123
        let base: u32 = if first == '0' {
            self.bump();
            match self.peek() {
                None => return Some(TokenKind::Literal(Literal::Integer(0u8.into()))),
                Some('x') => {
                    self.bump();
                    start = self.position;
                    16
                }
                Some('d') => {
                    self.bump();
                    start = self.position;
                    10
                }
                Some('o') => {
                    self.bump();
                    start = self.position;
                    8
                }
                Some('b') => {
                    self.bump();
                    start = self.position;
                    2
                }
                Some(_) => 10,
            }
        } else {
            10
        };
        let digit_pred: fn(char) -> bool = match base {
            16 => |c: char| c.is_ascii_hexdigit(),
            8 => |c: char| c.is_digit(8),
            2 => |c: char| c == '0' || c == '1',
            _ => |c: char| c.is_ascii_digit(),
        };
        self.skip_while(digit_pred);
        // Fractional part (decimal only). Consume the '.' only when a digit
        // follows so `1..5` still lexes as `1`, `..`, `5`.
        let mut is_float = false;
        if base == 10
            && self.peek() == Some('.')
            && self.peek_second().is_some_and(|c| c.is_ascii_digit())
        {
            is_float = true;
            self.bump(); // consume '.'
            self.skip_while(|c| c.is_ascii_digit());
        }
        let num_str = &self.input[start..self.position];
        if is_float {
            Some(TokenKind::Literal(Literal::Float(num_str.to_owned())))
        } else {
            let value =
                match <num_bigint::BigUint as num_traits::Num>::from_str_radix(num_str, base) {
                    Ok(v) => v,
                    Err(e) => {
                        return Some(TokenKind::Error(format!(
                            "Invalid integer literal '{}': {}",
                            num_str, e
                        )));
                    }
                };
            Some(TokenKind::Literal(Literal::Integer(value)))
        }
    }
}
