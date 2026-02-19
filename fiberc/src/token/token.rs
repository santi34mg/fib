use crate::token::keyword::Keyword;
use crate::token::literal::Literal;
use crate::token::operator::Operator;
use crate::token::punctuation::Punctuation;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // The identifier token contains the name of the identifier as a string
    Identifier(String),
    Literal(Literal),
    Keyword(Keyword),
    Operator(Operator),
    Punctuation(Punctuation),
    Unknown(char),
    Comment,
}
#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub line: usize,
    pub column: usize,
}

impl Token {
    pub fn new(kind: TokenKind, line: usize, column: usize) -> Self {
        return Self { kind, line, column };
    }
}
