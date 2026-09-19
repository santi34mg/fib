use std::fmt;

use crate::diagnostics::Span;
use crate::frontend::{
    ast::{enum_variant::EnumVariant, field::Field},
    identifier::Identifier,
    tokens::builtin::BuiltinType,
};

/// A syntax type expression together with the source position it starts on.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeExpression {
    pub kind: TypeExpressionKind,
    pub span: Span,
}

impl TypeExpression {
    pub const fn at(kind: TypeExpressionKind, span: Span) -> Self {
        Self { kind, span }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeExpressionKind {
    Builtin(BuiltinType),
    Identifier(Identifier),
    Function {
        argument_types: Vec<TypeExpression>,
        return_type: Box<TypeExpression>,
    },
    Tuple {
        elements: Vec<TypeExpression>,
    },
    Pointer {
        pointed_type: Box<TypeExpression>,
    },
    Struct {
        fields: Vec<Field>,
    },
    Enum {
        variants: Vec<EnumVariant>,
    },
    Array {
        element_type: Box<TypeExpression>,
        size: u64,
    },
    QualifiedIdentifier {
        module: Identifier,
        name: Identifier,
    },
    /// The `type` keyword used as a type annotation — indicates this binding holds a compile-time type value.
    TypeKeyword,
}
impl fmt::Display for TypeExpressionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeExpressionKind::Builtin(builtin_type) => {
                write!(f, "{}", builtin_type)
            }
            TypeExpressionKind::Identifier(identifier) => {
                write!(f, "{}", identifier)
            }
            TypeExpressionKind::Function {
                argument_types,
                return_type,
            } => {
                write!(f, "function({:?}) -> {}", argument_types, return_type)
            }
            TypeExpressionKind::Struct { fields } => {
                write!(f, "struct {{ {:?} }}", fields)
            }
            TypeExpressionKind::Tuple { elements } => {
                write!(f, "({:?})", elements)
            }
            TypeExpressionKind::Enum { variants } => {
                write!(f, "enum {{ {:?} }}", variants)
            }
            TypeExpressionKind::Array { element_type, size } => {
                write!(f, "{}[{}]", element_type, size)
            }
            TypeExpressionKind::QualifiedIdentifier { module, name } => {
                write!(f, "{}::{}", module, name)
            }
            TypeExpressionKind::TypeKeyword => {
                write!(f, "type")
            }
            TypeExpressionKind::Pointer { pointed_type } => {
                write!(f, "*{}", *pointed_type)
            }
        }
    }
}

impl fmt::Display for TypeExpression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.kind.fmt(f)
    }
}
