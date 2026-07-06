use crate::frontend::{
    identifier::Identifier,
    tokens::{Literal, Operator, builtin::BuiltinFunction},
    ast::type_expression::TypeExpression,
};

#[derive(Debug, Clone)]
pub enum Expression {
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
