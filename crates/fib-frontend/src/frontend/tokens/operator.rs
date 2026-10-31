#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    Def,          // :=
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

    /// Slice-range separator (`..` in `arr.[a..b]`, `[a..]`, `[..b]`, `[..]`).
    /// The inclusive end is spelled `.=` (`arr.[a.=b]`, `arr.[.=b]`).
    DoubleDot, // ..
    DotEq,          // .=
    Ellipsis,       // ...
    ThinRightArrow, // ->
    AddressOf,      // .&
    Dereference,    // .*
}
