use crate::tokens::builtin::Builtin;
use crate::tokens::identifier::Identifier;
use crate::tokens::keyword::Keyword;
use crate::tokens::literal::Literal;
use crate::tokens::operator::Operator;
use crate::tokens::punctuation::Punctuation;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // The identifier token contains the name of the identifier as a string
    Identifier(Identifier),
    Builtin(Builtin),
    Literal(Literal),
    Keyword(Keyword),
    Operator(Operator),
    Punctuation(Punctuation),
    Unknown(char),
    Comment,
    /// Produced by the lexer instead of panicking when invalid input is encountered.
    Error(String),
}
#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

impl Token {
    pub fn new(kind: TokenKind, line: usize, column: usize) -> Self {
        Self {
            kind,
            line,
            column,
            end_line: line,
            end_column: column,
        }
    }

    pub fn with_end(
        kind: TokenKind,
        line: usize,
        column: usize,
        end_line: usize,
        end_column: usize,
    ) -> Self {
        Self {
            kind,
            line,
            column,
            end_line,
            end_column,
        }
    }
}
