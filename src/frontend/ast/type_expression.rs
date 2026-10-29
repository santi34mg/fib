use std::fmt;

use crate::diagnostics::Span;
use crate::frontend::ast::qualified_name::QualifiedIdentifier;
use crate::frontend::{
    analysis_annotation::TypeAnnotation,
    ast::{enum_variant::EnumVariant, structl_field::StructField},
    tokens::builtin::BuiltinType,
};

/// A syntax type expression together with the source position it starts on.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeExpression {
    pub kind: TypeExpressionKind,
    pub span: Span,
    pub annotation: TypeAnnotation,
}

impl TypeExpression {
    pub const fn at(kind: TypeExpressionKind, span: Span) -> Self {
        Self {
            kind,
            span,
            annotation: TypeAnnotation { resolved: None },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeExpressionKind {
    /// A type-returning call, evaluated by the ordinary bounded type evaluator.
    Computed(Box<crate::frontend::ast::expression::Expression>),
    /// Readonly pointee/elements, independent of binding mutability.
    Readonly(Box<TypeExpression>),
    Builtin(BuiltinType),
    Function {
        calling_convention: crate::frontend::semantic_type::CallConvention,
        is_comp: bool,
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
    QualifiedIdentifier(QualifiedIdentifier),
}
impl fmt::Display for TypeExpressionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeExpressionKind::Computed(expression) => write!(f, "{:?}", expression.kind),
            TypeExpressionKind::Readonly(ty) => write!(f, "const {ty}"),
            TypeExpressionKind::Builtin(builtin_type) => f.write_str(builtin_type.name()),
            TypeExpressionKind::Function {
                calling_convention,
                is_comp,
                argument_types,
                return_type,
            } => {
                write!(
                    f,
                    "{}{}fn({:?}) {}",
                    if *calling_convention == crate::frontend::semantic_type::CallConvention::C {
                        "callconv \"C\" "
                    } else {
                        ""
                    },
                    if *is_comp { "comp " } else { "" },
                    argument_types,
                    return_type
                )
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
                if matches!(
                    element_type.kind,
                    TypeExpressionKind::Pointer { .. } | TypeExpressionKind::Readonly(_)
                ) {
                    write!(f, "({element_type})[{size}]")
                } else {
                    write!(f, "{element_type}[{size}]")
                }
            }
            TypeExpressionKind::Slice { element_type } => {
                if matches!(
                    element_type.kind,
                    TypeExpressionKind::Pointer { .. } | TypeExpressionKind::Readonly(_)
                ) {
                    write!(f, "({element_type})[]")
                } else {
                    write!(f, "{element_type}[]")
                }
            }
            TypeExpressionKind::QualifiedIdentifier(QualifiedIdentifier { segments, .. }) => {
                if let Some((last, rest)) = segments.split_last() {
                    for sym in rest {
                        write!(f, "{}::", sym)?
                    }
                    write!(f, "{}", last)?
                }
                Ok(())
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
