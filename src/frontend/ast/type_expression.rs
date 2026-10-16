use std::fmt;

use crate::diagnostics::Span;
use crate::frontend::ast::qualified_name::QualifiedName;
use crate::frontend::tokens::name::Name;
use crate::frontend::{
    ast::{enum_variant::EnumVariant, structl_field::StructField},
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
        fields: Vec<StructField>,
    },
    Enum {
        variants: Vec<EnumVariant>,
    },
    Array {
        element_type: Box<TypeExpression>,
        size: u64,
    },
    /// A slice type `T[]` — a runtime `(ptr, len)` view over element type T.
    Slice {
        element_type: Box<TypeExpression>,
    },
    UnqualifiedName(Name),
    QualifiedName(QualifiedName),
    /// The `type` keyword used as a type annotation — indicates this binding holds a compile-time type value.
    TypeKeyword,
}
impl fmt::Display for TypeExpressionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeExpressionKind::Builtin(builtin_type) => f.write_str(builtin_type.name()),
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
            TypeExpressionKind::Slice { element_type } => {
                write!(f, "{}[]", element_type)
            }
            TypeExpressionKind::QualifiedName(QualifiedName { segments }) => {
                if let Some((last, rest)) = segments.split_last() {
                    for sym in rest {
                        write!(f, "{}::", sym)?
                    }
                    write!(f, "{}", last)?
                }
                Ok(())
            }
            TypeExpressionKind::TypeKeyword => {
                write!(f, "type")
            }
            TypeExpressionKind::Pointer { pointed_type } => {
                write!(f, "*{}", *pointed_type)
            }
            TypeExpressionKind::UnqualifiedName(name) => {
                write!(f, "{}", name)
            }
        }
    }
}

impl fmt::Display for TypeExpression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.kind.fmt(f)
    }
}
