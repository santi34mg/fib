use core::panic;
use std::char;

use crate::token::{
    Keyword, Literal, Operator, Punctuation, Token, TokenKind,
    builtin::{Builtin, BuiltinType},
    identifier::Identifier,
};

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
                        Some(TokenKind::Operator(Operator::PlusAssign))
                    }
                    Some(_) => Some(TokenKind::Operator(Operator::Plus)),
                    None => todo!(),
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
                        Some(TokenKind::Operator(Operator::StarAssign))
                    }
                    Some(_) => Some(TokenKind::Operator(Operator::Star)),
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
                        Some(TokenKind::Comment)
                    }
                    Some('=') => {
                        self.bump();
                        Some(TokenKind::Operator(Operator::SlashAssign))
                    }
                    Some(_) => Some(TokenKind::Operator(Operator::Slash)),
                    None => todo!(),
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
                    Some(_) => Some(TokenKind::Operator(Operator::Percent)),
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
                    Some(TokenKind::Operator(Operator::DoubleDot))
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
            '\'' => self.lex_char_literal(),
            '[' => {
                self.bump();
                Some(TokenKind::Punctuation(Punctuation::OpeningSquareBrace))
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
                self.bump(); // consume closing quote
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

    fn lex_char_literal(&mut self) -> Option<TokenKind> {
        // consume opening quote
        self.bump();
        // get the character
        let ch = self.bump()?;
        // if char is '\\' then it is an escape sequence
        let ch = if ch == '\\' {
            let esc = self.bump()?;
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
                        let c = self.bump()?;
                        if c.is_ascii_hexdigit() {
                            hex.push(c);
                        } else {
                            panic!("Invalid hex escape sequence");
                        }
                    }
                    let value = u8::from_str_radix(&hex, 16).unwrap_or_else(|e| {
                        eprintln!("Error: {}\nfor hex string \"{}\"", e, hex);
                        panic!();
                    });
                    value as char
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
                                panic!("Invalid unicode escape sequence");
                            }
                        }
                        self.bump(); // consume '}'
                        let value = u32::from_str_radix(&hex, 16).unwrap_or_else(|e| {
                            eprintln!("Error: {}\nfor unicode hex string \"{}\"", e, hex);
                            panic!();
                        });
                        char::from_u32(value).unwrap_or_else(|| {
                            panic!("Invalid unicode code point: {}", value);
                        })
                    } else {
                        panic!("Invalid unicode escape sequence");
                    }
                }
                c => {
                    panic!("Invalid escape sequence: \\{}", c);
                }
            }
        } else {
            ch
        };
        self.bump(); // consume closing quote
        Some(TokenKind::Literal(Literal::Character(ch)))
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
                _c => {
                    // anything else means its just one digit
                    (10, |c: char| c.is_ascii_digit() || c == '.')
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
            let value = u64::from_str_radix(num_str, base).unwrap_or_else(|e| {
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
        // for now, "__<identifier>" is reserved for internal identifiers
        match name {
            "const" => TokenKind::Keyword(Keyword::Const),
            "var" => TokenKind::Keyword(Keyword::Var),
            "fn" => TokenKind::Keyword(Keyword::Function),
            "switch" => TokenKind::Keyword(Keyword::Switch),
            "when" => TokenKind::Keyword(Keyword::When),
            "struct" => TokenKind::Keyword(Keyword::Struct),
            "enum" => TokenKind::Keyword(Keyword::Enum),
            "union" => TokenKind::Keyword(Keyword::Union),
            "if" => TokenKind::Keyword(Keyword::If),
            "else" => TokenKind::Keyword(Keyword::Else),
            "for" => TokenKind::Keyword(Keyword::For),
            "break" => TokenKind::Keyword(Keyword::Break),
            "continue" => TokenKind::Keyword(Keyword::Continue),
            "return" => TokenKind::Keyword(Keyword::Return),
            "true" => TokenKind::Literal(Literal::Boolean(true)),
            "false" => TokenKind::Literal(Literal::Boolean(false)),
            "null" => TokenKind::Literal(Literal::Null),
            "void" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::Void)),
            "uint8" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::UInt8)),
            "uint16" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::UInt16)),
            "uint32" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::UInt32)),
            "uint64" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::UInt64)),
            "sint8" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::SInt8)),
            "sint16" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::SInt16)),
            "sint32" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::SInt32)),
            "sint64" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::SInt64)),
            "float32" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::Float32)),
            "float64" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::Float64)),
            "char" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::Char)),
            "bool" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::Boolean)),
            _ => TokenKind::Identifier(Identifier {
                identifier: name.to_string(),
            }),
        }
    }
}

impl<'input> Iterator for Lexer<'input> {
    type Item = Token;

    fn next(&mut self) -> Option<Self::Item> {
        self.lex_token()
    }
}
