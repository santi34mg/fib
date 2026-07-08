#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Operator {
    Plus,    // +
    Minus,   // -
    Star,    // *
    Slash,   // /
    Percent, // %

    GreaterThan,  // >
    LesserThan,   // <
    GreaterEqual, // >=
    LesserEqual,  // <=

    LeftShift,  // <<
    RightShift, // >>

    Assign,       // =
    DoubleEquals, // ==
    Different,    // !=

    PlusAssign,    // +=
    MinusAssign,   // -=
    StarAssign,    // *=
    SlashAssign,   // /=
    PercentAssign, // %=

    LogicalAnd, // &&
    LogicalOr,  // ||
    LogicalNot, // !

    Ampersand, // &
    Pipe,      // |
    Tilde,     // ~
    Caret,     // ^

    /// Reserved for future range syntax (e.g. `0..n`); not consumed by the parser yet.
    DoubleDot, // ..
    Ellipsis,       // ...
    ThinRightArrow, // ->
}
