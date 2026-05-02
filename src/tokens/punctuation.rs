#[derive(Debug, Clone, PartialEq)]
pub enum Punctuation {
    OpeningParenthesis,
    ClosingParenthesis,
    OpeningCurlyBrace,
    ClosingCurlyBrace,
    OpeningSquareBrace,
    ClosingSquareBrace,
    Semicolon,
    Comma,
    Colon,
    DoubleColon,
    Dot,
    /// Reserved for future attribute/decorator syntax; not consumed by the parser yet.
    At,
}
