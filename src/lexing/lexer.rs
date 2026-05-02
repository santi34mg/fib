use std::char;

use crate::tokens::{
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
                    _ => Some(TokenKind::Operator(Operator::Slash)),
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
                    if self.peek() == Some('.') {
                        self.bump();
                        Some(TokenKind::Operator(Operator::Ellipsis))
                    } else {
                        Some(TokenKind::Operator(Operator::DoubleDot))
                    }
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
                if self.peek() == Some(':') {
                    self.bump();
                    Some(TokenKind::Punctuation(Punctuation::DoubleColon))
                } else {
                    Some(TokenKind::Punctuation(Punctuation::Colon))
                }
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
                let mut escaped = String::new();
                let mut lex_error: Option<&str> = None;
                loop {
                    match self.peek() {
                        None => {
                            lex_error = Some("unterminated string literal");
                            break;
                        }
                        Some('\"') => {
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
                                Some('\"') => escaped.push('\"'),
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
            '@' => {
                self.bump();
                Some(TokenKind::Punctuation(Punctuation::At))
            }
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
                            "Invalid unicode escape sequence: expected '{{'".into(),
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
        self.bump(); // consume closing quote
        Some(TokenKind::Literal(Literal::Character(ch)))
    }

    fn lex_numeric(&mut self, first: char) -> Option<TokenKind> {
        let mut start = self.position;
        // first char is 0 might be 0x... or might be 0123
        let (base, f): (u32, fn(char) -> bool) = if first == '0' {
            self.bump();
            let second = match self.peek() {
                Some(c) => c,
                None => return Some(TokenKind::Literal(Literal::Integer(0))),
            };
            match second {
                'x' => {
                    self.bump();
                    start = self.position;
                    (16, |c: char| c.is_ascii_hexdigit())
                }
                'd' => {
                    self.bump();
                    start = self.position;
                    (10, |c: char| c.is_ascii_digit())
                }
                'o' => {
                    self.bump();
                    start = self.position;
                    (8, |c: char| c.is_digit(8))
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
        self.skip_while(f);
        let num_str = &self.input[start..self.position];
        if num_str.contains('.') {
            let value = ("0".to_string() + num_str).parse::<f64>().ok()?;
            Some(TokenKind::Literal(Literal::Float(value)))
        } else {
            let value = match u64::from_str_radix(num_str, base) {
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

    fn lex_identifier_or_keyword(&mut self) -> TokenKind {
        let start = self.position;
        self.skip_while(|c| c.is_alphanumeric() || c == '_');
        let name = &self.input[start..self.position];
        // for now, "__<identifier>" is reserved for internal identifiers
        match name {
            "var" => TokenKind::Keyword(Keyword::Var),
            "fn" => TokenKind::Keyword(Keyword::Function),
            "switch" => TokenKind::Keyword(Keyword::Switch),
            "when" => TokenKind::Keyword(Keyword::When),
            "type" => TokenKind::Keyword(Keyword::Type),
            "struct" => TokenKind::Keyword(Keyword::Struct),
            "enum" => TokenKind::Keyword(Keyword::Enum),
            "union" => TokenKind::Keyword(Keyword::Union),
            "if" => TokenKind::Keyword(Keyword::If),
            "else" => TokenKind::Keyword(Keyword::Else),
            "for" => TokenKind::Keyword(Keyword::For),
            "break" => TokenKind::Keyword(Keyword::Break),
            "continue" => TokenKind::Keyword(Keyword::Continue),
            "ret" => TokenKind::Keyword(Keyword::Return),
            "as" => TokenKind::Keyword(Keyword::As),
            "extern" => TokenKind::Keyword(Keyword::Extern),
            "defer" => TokenKind::Keyword(Keyword::Defer),
            "import" => TokenKind::Keyword(Keyword::Import),
            "true" => TokenKind::Literal(Literal::Boolean(true)),
            "false" => TokenKind::Literal(Literal::Boolean(false)),
            "null" => TokenKind::Literal(Literal::Null),
            "void" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::Void)),
            "uint" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::UInt1)),
            "uint2" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::UInt2)),
            "uint4" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::UInt4)),
            "uint8" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::UInt8)),
            "uint16" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::UInt16)),
            "int" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::Int1)),
            "int2" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::Int2)),
            "int4" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::Int4)),
            "int8" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::Int8)),
            "int16" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::Int16)),
            "float4" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::Float4)),
            "float8" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::Float8)),
            "float16" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::Float16)),
            "string" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::String)),
            "char" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::Char)),
            "bool" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::Boolean)),
            "never" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::Never)),
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
