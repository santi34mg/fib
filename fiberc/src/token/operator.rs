#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Operator {
    Plus,
    Minus,
    Multiply,
    Divide,
    Modulo,

    StructuralEquals,
    StrictlyEquals,

    StructuralDifferent,
    StrictlyDifferent,

    GreaterThan,
    LesserThan,
    GreaterEqual,
    LesserEqual,

    Assign,
    AddAssign,
    MinusAssign,
    MultiplyAssign,
    DivideAssign,
    ModuloAssign,

    LogicalAnd,
    LogicalOr,
    LogicalNot,

    Ampersand,
    Pipe,
    Tilde,
    Caret,

    LeftShift,
    RightShift,

    Range,

    TypeReturn,
}
