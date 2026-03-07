#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Operator {
    // Integers 
    Plus,
    Minus,
    Multiply,
    Divide,
    Modulo,

    GreaterThan,
    LesserThan,
    GreaterEqual,
    LesserEqual,

    LeftShift,
    RightShift,

    // 
    StructuralEquals,
    StrictlyEquals,

    StructuralDifferent,
    StrictlyDifferent,

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

    Range,

    TypeReturn,
}
