use std::fmt;

use crate::frontend::{ast::{enum_variant::EnumVariant, field::Field}, identifier::Identifier, tokens::builtin::BuiltinType};

#[derive(Debug, Clone, PartialEq)]
pub enum TypeExpression {
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
impl fmt::Display for TypeExpression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeExpression::Builtin(builtin_type) => {
                write!(f, "{}", builtin_type)
            }
            TypeExpression::Identifier(identifier) => {
                write!(f, "{}", identifier)
            }
            TypeExpression::Function {
                argument_types,
                return_type,
            } => {
                write!(f, "function({:?}) -> {}", argument_types, return_type)
            }
            TypeExpression::Struct { fields } => {
                write!(f, "struct {{ {:?} }}", fields)
            }
            TypeExpression::Tuple { elements } => {
                write!(f, "({:?})", elements)
            }
            TypeExpression::Enum { variants } => {
                write!(f, "enum {{ {:?} }}", variants)
            }
            TypeExpression::Array { element_type, size } => {
                write!(f, "{}[{}]", element_type, size)
            }
            TypeExpression::QualifiedIdentifier { module, name } => {
                write!(f, "{}::{}", module, name)
            }
            TypeExpression::TypeKeyword => {
                write!(f, "type")
            }
            TypeExpression::Pointer {
                pointed_type,
            } => {
                write!(f, "*{}", *pointed_type)
            },
        }
    }
}

