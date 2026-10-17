//! Analysis slots embedded in the original syntax tree, not a second typed AST.
//!
//! Parsing leaves each slot `None`. Name resolution fills name/declaration/label
//! slots; type resolution fills type/expression/member slots. `None` means pending,
//! never a successful unknown type. Before IR lowering, analysis must validate all
//! applicable slots. Source spellings and spans remain on the original AST nodes.
//!
//! `Expression`, `QualifiedIdentifier`, and `TypeExpression` own expression,
//! name, and type slots respectively. `ExprAcess` and enum patterns own member
//! slots; `StructField` owns one for both initializer and type-definition fields.
//! Function bindings live only on `FunctionSignature`, not its declaration
//! wrapper. Parameters, type/variable declarations (including each multi-binding),
//! and `PatternBinding` each own one binding slot. Members are never bindings.
//!
//! `LabelAnnotation` is not embedded yet: label syntax is undefined. The reserved
//! `FunctionSignature::generic_parameters` string list is not parsed yet either;
//! once its syntax is defined, each parameter needs its own binding-bearing node.
//! The legacy, unproduced `ExpressionKind::QualifiedAccess` variant has no name
//! slot: parsed paths use the annotated `QualifiedIdentifier` instead.

use crate::frontend::{
    identifier::{DeclId, LabelId, LocalId, ResolvedName},
    semantic_type::TypeId,
};

/// Attach to a name use, including the final target of a qualified path.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NameAnnotation {
    pub resolved: Option<ResolvedName>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingId {
    Declaration(DeclId),
    Local(LocalId),
}

/// Attach to binding sites, including parameters and pattern bindings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BindingAnnotation {
    pub binding: Option<BindingId>,
}

/// Reserved for label declarations and control-flow target uses, not ordinary
/// names. Not embedded until label syntax is defined.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LabelAnnotation {
    pub target: Option<LabelId>,
}

/// The type denoted by a syntax type expression (not the metatype of a value).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TypeAnnotation {
    pub resolved: Option<TypeId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueCategory {
    Value,
    ImmutablePlace,
    MutablePlace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExpressionInfo {
    pub ty: TypeId,
    pub category: ValueCategory,
    /// A type-valued expression has metatype `ty` and denotes this type.
    pub type_value: Option<TypeId>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExpressionAnnotation {
    pub resolved: Option<ExpressionInfo>,
}

/// Member indices refer to the owner's semantic field/variant list, not locals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberKind {
    Field(u32),
    EnumVariant(u32),
    Length,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemberInfo {
    pub owner: TypeId,
    pub kind: MemberKind,
    /// Result type of selection (a payload variant may be a constructor).
    pub ty: TypeId,
}

/// Attach to field accesses, struct initializer fields, and variant selections.
/// Resolving a member never creates or rewrites a lexical binding.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MemberAnnotation {
    pub resolved: Option<MemberInfo>,
}
