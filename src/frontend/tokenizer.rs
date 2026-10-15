use std::char;

use crate::frontend::tokens::{Token, TokenKind};

pub struct Tokenizer<'input> {
    input: &'input str,
    position: usize,
    line: usize,
    column: usize,
    last_identifier_id: u64,
}

impl<'input> Tokenizer<'input> {
    pub fn new(input: &'input str) -> Self {
        Self {
            input,
            position: 0,
            line: 1,
            column: 1,
            last_identifier_id: 0,
        }
    }

    fn peek(&self) -> Option<char> {
        self.input[self.position..].chars().next()
    }

    fn peek_second(&self) -> Option<char> {
        self.input[self.position..].chars().nth(1)
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        if c == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        self.position += c.len_utf8();
        Some(c)
    }

    fn skip_while<F>(&mut self, mut pred: F)
    where
        F: FnMut(char) -> bool,
    {
        while let Some(c) = self.peek() {
            if pred(c) {
                self.bump();
            } else {
                break;
            }
        }
    }

    fn skip_whitespace(&mut self) {
        self.skip_while(|c| c.is_whitespace());
    }

    fn lex_token(&mut self) -> Option<Token> {
        self.skip_whitespace();
        let start_line = self.line;
        let start_col = self.column;

        let c = self.peek()?;
        let kind: Option<TokenKind> = match c {
            '=' | '!' | '>' | '<' | '+' | '-' | '*' | '%' | '&' | '|' | '^' | '~' => {
                self.lex_operator(c)
            }
            '/' => self.lex_slash(),
            '.' => self.lex_dot(),
            '(' | ')' | '{' | '}' | '[' | ']' | ',' | ';' | ':' | '@' => self.lex_punctuation(c),
            '"' => self.lex_string_literal(),
            '\'' => self.lex_char_literal(),
            c if c.is_ascii_digit() => self.lex_numeric(c),
            c if c.is_alphabetic() || c == '_' => Some(self.lex_identifier_or_keyword()),
            c => {
                self.bump();
                Some(TokenKind::Unknown(c))
            }
        };
        Some(Token::with_end(
            kind?,
            start_line,
            start_col,
            self.line,
            self.column.saturating_sub(1),
        ))
    }
}

impl<'i> Iterator for Tokenizer<'i> {
    type Item = Token;

    fn next(&mut self) -> Option<Self::Item> {
        self.lex_token()
    }
}

mod char_literal;
mod dot;
mod name_or_keyword;
mod numeric_literal;
mod operator;
mod punctuation;
mod slash;
mod string;
mod test;
