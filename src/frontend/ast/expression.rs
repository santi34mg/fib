use crate::diagnostics::Span;
use crate::frontend::ast::binary_expression::BinaryExpression;
use crate::frontend::ast::qualified_name::QualifiedIdentifier;
use crate::frontend::ast::structl_field::StructField;
use crate::frontend::ast::unary_expression::UnaryExpression;
use crate::frontend::{
    analysis_annotation::{ExpressionAnnotation, MemberAnnotation},
    ast::type_expression::TypeExpression,
    tokens::Literal,
};

/// An expression together with the source position it starts on. The
/// analyzer records these spans so type errors can point at the exact
/// offending sub-expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Expression {
    pub kind: ExpressionKind,
    pub span: Span,
    pub annotation: ExpressionAnnotation,
}

impl Expression {
    pub const fn at(kind: ExpressionKind, span: Span) -> Self {
        Self {
            kind,
            span,
            annotation: ExpressionAnnotation { resolved: None },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ExpressionKind {
    /// Binary expressions like `1 + 1`
    BinaryExpresison(BinaryExpression),
    /// Unary expressions like `!true`
    UnaryExpression(UnaryExpression),
    /// Literals like `123`, `true`, `"abc"`
    Literal(Literal),
    /// A call is any expression that has `callee(args)`, this can be an enum construction, a function or method call
    Call {
        callee: Box<Expression>,
        args: Vec<Expression>,
    },
    QualifiedIdentifier(QualifiedIdentifier),
    /// Access to an expression's inner constructs. This can work for (`my_struct.my_field`)
    ExprAcess {
        expr: Box<Expression>,
        accessed_identifier: String,
        member: MemberAnnotation,
    },
    /// Construction of a struct
    StructConstruct {
        struct_identifier: QualifiedIdentifier,
        fields: Vec<StructField>,
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
    SliceLiteral {
        object: Box<Expression>,
        start: Option<Box<Expression>>,
        end: Option<Box<Expression>>,
        inclusive: bool,
    },
    ArrayLiteral {
        elements: Vec<Expression>,
    },
    /// A compile-time type value used in expression position (e.g., as a generic argument).
    TypeValue(TypeExpression),
    /// The void expression simply means do nothing
    VoidExpression,
}
