use core::panic;
use std::char;

use crate::token::{Keyword, Literal, Operator, Punctuation, Token, TokenKind};

pub struct Lexer<'input> {
    input: &'input str,
    position: usize,
    line: usize,
    column: usize,
}

impl<'input> Lexer<'input> {
    pub fn new(input: &'input str) -> Self {
        Self {
            input,
            position: 0,
            line: 1,
            column: 1,
        }
    }

    fn peek(&self) -> Option<char> {
        self.input[self.position..].chars().next()
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
            '=' => {
                self.bump();
                if self.peek() == Some('=') {
                    self.bump();
                    if self.peek() == Some('=') {
                        self.bump();
                        Some(TokenKind::Operator(Operator::StrictlyEquals))
                    } else {
                        Some(TokenKind::Operator(Operator::StructuralEquals))
                    }
                } else {
                    Some(TokenKind::Operator(Operator::Assign))
                }
            }
            '!' => {
                self.bump();
                if self.peek() == Some('=') {
                    self.bump();
                    if self.peek() == Some('=') {
                        self.bump();
                        Some(TokenKind::Operator(Operator::StrictlyDifferent))
                    } else {
                        Some(TokenKind::Operator(Operator::StructuralDifferent))
                    }
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
                    Some(_) => Some(TokenKind::Operator(Operator::GreaterThan)),
                    None => todo!(),
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
                    Some(_) => Some(TokenKind::Operator(Operator::LesserThan)),
                    None => todo!(),
                }
            }
            '+' => {
                self.bump();
                let c = self.peek();
                match c {
                    Some('=') => {
                        self.bump();
                        Some(TokenKind::Operator(Operator::AddAssign))
                    }
                    Some(_) => Some(TokenKind::Operator(Operator::Minus)),
                    None => todo!(),
                }
            }
            '-' => {
                self.bump();
                let c = self.peek();
                match c {
                    Some('>') => {
                        self.bump();
                        Some(TokenKind::Operator(Operator::TypeReturn))
                    }
                    Some('=') => {
                        self.bump();
                        Some(TokenKind::Operator(Operator::MinusAssign))
                    }
                    Some(_) => Some(TokenKind::Operator(Operator::Minus)),
                    None => todo!(),
                }
            }
            '*' => {
                self.bump();
                let c = self.peek();
                match c {
                    Some('=') => {
                        self.bump();
                        Some(TokenKind::Operator(Operator::MultiplyAssign))
                    }
                    Some(_) => Some(TokenKind::Operator(Operator::Multiply)),
                    None => todo!(),
                }
            }
            '/' => {
                self.bump();
                let c = self.peek();
                match c {
                    Some('/') => {
                        self.bump();
                        self.skip_while(|c| c != '\n');
                        None
                    }
                    Some('=') => {
                        self.bump();
                        Some(TokenKind::Operator(Operator::DivideAssign))
                    }
                    Some(_) => Some(TokenKind::Operator(Operator::Divide)),
                    None => todo!(),
                }
            }
            '%' => {
                self.bump();
                let c = self.peek();
                match c {
                    Some('=') => {
                        self.bump();
                        Some(TokenKind::Operator(Operator::ModuloAssign))
                    }
                    Some(_) => Some(TokenKind::Operator(Operator::Modulo)),
                    None => todo!(),
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
            ',' => {
                self.bump();
                Some(TokenKind::Punctuation(Punctuation::Comma))
            }
            '.' => {
                self.bump();
                if self.peek() == Some('.') {
                    self.bump();
                    Some(TokenKind::Operator(Operator::Range))
                } else {
                    Some(TokenKind::Punctuation(Punctuation::Dot))
                }
            }
            ';' => {
                self.bump();
                Some(TokenKind::Punctuation(Punctuation::Semicolon))
            }
            ':' => {
                self.bump();
                Some(TokenKind::Punctuation(Punctuation::Colon))
            }
            '\'' => {
                self.bump(); // consume opening quote
                let ch = self.bump()?; // get the character
                if self.bump()? != '\'' {
                    // expect closing quote
                    return Some(Token::new(TokenKind::Unknown(ch), start_line, start_col));
                }
                Some(TokenKind::Literal(Literal::Character(ch)))
            }
            '[' => {
                self.bump();
                Some(TokenKind::Punctuation(Punctuation::OpenSquareBrace))
            }
            ']' => {
                self.bump();
                Some(TokenKind::Punctuation(Punctuation::ClosingSquareBrace))
            }
            '\"' => {
                self.bump();
                let starting_pos = self.position;
                self.skip_while(|c| c != '\"');
                let s: &str = &self.input[starting_pos..self.position];
                Some(TokenKind::Literal(Literal::String(s.to_string())))
            }
            '@' => {
                self.bump();
                Some(TokenKind::Punctuation(Punctuation::At))
            }
            c if c.is_ascii_digit() => self.lex_numeric(c),
            c if c.is_alphabetic() => Some(self.lex_identifier_or_keyword()),
            c => {
                self.bump();
                Some(TokenKind::Unknown(c))
            }
        };
        Some(Token::new(kind?, start_line, start_col))
    }

    fn lex_numeric(&mut self, first: char) -> Option<TokenKind> {
        let mut start = self.position;
        // first char is 0 might be 0x... or might be 0123
        let (base, f): (u32, fn(char) -> bool) = if first == '0' {
            self.bump();
            let second = self.peek()?;
            match second {
                'x' => {
                    self.bump();
                    start = self.position;
                    (16, |c: char| c.is_ascii_hexdigit())
                }
                'd' => {
                    self.bump();
                    (10, |c: char| c.is_ascii_digit())
                }
                'o' => {
                    self.bump();
                    start = self.position;
                    (8, |c: char| c == '0' || c == '1')
                }
                'b' => {
                    self.bump();
                    start = self.position;
                    (2, |c: char| c == '0' || c == '1')
                }
                c if c.is_ascii_digit() => (10, |c: char| c.is_ascii_digit() || c == '.'),
                '.' => (10, |c: char| c.is_ascii_digit() || c == '.'),
                // TODO: better error handling
                _ => {
                    panic!()
                }
            }
        } else {
            (10, |c: char| c.is_ascii_digit() || c == '.')
        };
        self.skip_while(|c| f(c));
        let num_str = &self.input[start..self.position];
        if num_str.contains('.') {
            let value = ("0".to_string() + num_str).parse::<f32>().ok()?;
            return Some(TokenKind::Literal(Literal::Float(value)));
        } else {
            let value = u32::from_str_radix(num_str, base).unwrap_or_else(|e| {
                eprintln!("Error: {}\nfor string \"{}\"", e, num_str);
                // TODO: better errors
                panic!();
            });
            return Some(TokenKind::Literal(Literal::Integer(value)));
        }
    }

    fn lex_identifier_or_keyword(&mut self) -> TokenKind {
        let start = self.position;
        self.skip_while(|c| c.is_alphanumeric() || c == '_');
        let name = &self.input[start..self.position];
        match name {
            "module" => TokenKind::Keyword(Keyword::Module),
            "use" => TokenKind::Keyword(Keyword::Use),
            "public" => TokenKind::Keyword(Keyword::Public),
            "private" => TokenKind::Keyword(Keyword::Private),
            "let" => TokenKind::Keyword(Keyword::Let),
            "mut" => TokenKind::Keyword(Keyword::Mutable),
            "function" => TokenKind::Keyword(Keyword::Function),
            "match" => TokenKind::Keyword(Keyword::Match),
            "when" => TokenKind::Keyword(Keyword::When),
            "coroutine" => TokenKind::Keyword(Keyword::Coroutine),
            "spawn" => TokenKind::Keyword(Keyword::Spawn),
            "resume" => TokenKind::Keyword(Keyword::Resume),
            "addressof" => TokenKind::Keyword(Keyword::Addressof),
            "deref" => TokenKind::Keyword(Keyword::Dereference),
            "contract" => TokenKind::Keyword(Keyword::Contract),
            "impl" => TokenKind::Keyword(Keyword::Implementation),
            "type" => TokenKind::Keyword(Keyword::Type),
            "struct" => TokenKind::Keyword(Keyword::Struct),
            "variant" => TokenKind::Keyword(Keyword::Variant),
            "if" => TokenKind::Keyword(Keyword::If),
            "else" => TokenKind::Keyword(Keyword::Else),
            "for" => TokenKind::Keyword(Keyword::For),
            "break" => TokenKind::Keyword(Keyword::Break),
            "continue" => TokenKind::Keyword(Keyword::Continue),
            "return" => TokenKind::Keyword(Keyword::Return),
            "dynamic" => TokenKind::Keyword(Keyword::Dynamic),
            "blob" => TokenKind::Keyword(Keyword::Blob),
            "never" => TokenKind::Keyword(Keyword::Never),
            "int" => TokenKind::Keyword(Keyword::Integer),
            "float" => TokenKind::Keyword(Keyword::Float),
            "string" => TokenKind::Keyword(Keyword::String),
            "bool" => TokenKind::Keyword(Keyword::Boolean),
            "char" => TokenKind::Keyword(Keyword::Character),
            "unit" => TokenKind::Keyword(Keyword::Unit),
            "unique" => TokenKind::Keyword(Keyword::Unique),
            "shared" => TokenKind::Keyword(Keyword::Shared),
            "wak" => TokenKind::Keyword(Keyword::Weak),
            "true" => TokenKind::Literal(Literal::Boolean(true)),
            "false" => TokenKind::Literal(Literal::Boolean(false)),
            "null" => TokenKind::Literal(Literal::Null),
            _ => TokenKind::Identifier(name.to_string()),
        }
    }
}

impl<'input> Iterator for Lexer<'input> {
    type Item = Token;

    fn next(&mut self) -> Option<Self::Item> {
        self.lex_token()
    }
}
