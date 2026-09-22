use crate::diagnostics::Span;
use crate::frontend::{
    ast::type_expression::TypeExpression,
    identifier::Identifier,
    tokens::{Literal, Operator, builtin::BuiltinFunction},
};

/// An expression together with the source position it starts on. The
/// analyzer records these spans so type errors can point at the exact
/// offending sub-expression.
#[derive(Debug, Clone)]
pub struct Expression {
    pub kind: ExpressionKind,
    pub span: Span,
}

impl Expression {
    pub const fn at(kind: ExpressionKind, span: Span) -> Self {
        Self { kind, span }
    }
}

#[derive(Debug, Clone)]
pub enum ExpressionKind {
    Binary {
        left: Box<Expression>,
        operator: Operator,
        right: Box<Expression>,
    },
    Unary {
        operator: Operator,
        expression: Box<Expression>,
    },
    Literal(Literal),
    // The identifier expression contains the name of the identifier as a string
    Identifier(Identifier),
    Grouping(Box<Expression>),
    Call {
        callee: Box<Expression>,
        args: Vec<Expression>,
    },
    /// A call to a builtin function, e.g. `@concat(a, b)`.
    BuiltinCall {
        builtin: BuiltinFunction,
        args: Vec<Expression>,
    },
    FieldAccess {
        object: Box<Expression>,
        field: Identifier,
    },
    AddressOf(Box<Expression>),
    Dereference(Box<Expression>),
    StructConstruct {
        type_name: Identifier,
        fields: Vec<(Identifier, Expression)>,
    },
    Cast {
        expr: Box<Expression>,
        target_type: TypeExpression,
    },
    IndexAccess {
        object: Box<Expression>,
        index: Box<Expression>,
    },
    /// Slice of an array or slice as `T[]`.
    /// - `obj.[a..b]` → `[a, b)`, `obj.[a.=b]` → `[a, b]` (inclusive)
    /// - `obj.[a..]` → `[a, len)`, `obj.[..b]` → `[0, b)`,
    ///   `obj.[.=b]` → `[0, b]` (inclusive), `obj.[..]` → full range.
    ///
    /// `None` means an omitted bound; `inclusive` marks a `.=` end.
    Slice {
        object: Box<Expression>,
        start: Option<Box<Expression>>,
        end: Option<Box<Expression>>,
        inclusive: bool,
    },
    ArrayLiteral {
        elements: Vec<Expression>,
    },
    QualifiedAccess {
        module: Identifier,
        member: Identifier,
    },
    /// A compile-time type value used in expression position (e.g., as a generic argument).
    TypeValue(TypeExpression),
    /// `Type.Variant { field: val, ... }` — construct an enum variant carrying
    /// a payload. For payload-less variants, use a plain `FieldAccess` instead.
    EnumVariantConstruct {
        type_name: Identifier,
        variant: Identifier,
        fields: Vec<(Identifier, Expression)>,
    },
}
