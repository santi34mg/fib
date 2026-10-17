//! Semantic type contract shared by analysis annotations and IR lowering.
//! This is neither a second AST nor a type inference implementation.

use std::collections::HashMap;

use crate::frontend::{
    identifier::{DeclId, LocalId},
    tokens::builtin::BuiltinType,
};

/// An index in one analyzed project's type table; not a layout or ABI identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeId(pub u32);

#[derive(Debug, Clone, PartialEq)]
pub enum SemanticType {
    Builtin(BuiltinType),
    Pointer(TypeId),
    Array {
        element: TypeId,
        length: u64,
    },
    Slice(TypeId),
    Tuple(Vec<TypeId>),
    Function(FunctionType),
    Struct(Vec<FieldType>),
    Enum(Vec<VariantType>),
    /// A declaration-backed type. Its definition is in `declarations`.
    /// Recursive-type legality remains the responsibility of the type pass.
    Declared(DeclId),
    /// Generic parameters are identified by bindings, not their spelling.
    Parameter(LocalId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionType {
    pub parameters: Vec<TypeId>,
    pub return_type: TypeId,
    pub variadic: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldType {
    pub spelling: String,
    pub ty: TypeId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantType {
    pub spelling: String,
    pub payload: Option<TypeId>,
}

/// Owned by `AnalyzedProject`. Empty is a valid workspace, not proof of analysis.
/// Type interning, alias normalization, inference, and layout are not implemented.
#[derive(Debug, Clone, Default)]
pub struct SemanticTypes {
    pub types: Vec<SemanticType>,
    pub declarations: HashMap<DeclId, TypeId>,
    pub locals: HashMap<LocalId, TypeId>,
}
