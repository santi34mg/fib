//! Semantic type contract shared by analysis annotations and IR lowering.
//! This is neither a second AST nor a type inference implementation.

use crate::frontend::analysis_annotation::{BindingId, ExpressionInfo};
use std::collections::{HashMap, HashSet};

use crate::frontend::{
    identifier::{DeclId, LocalId},
    tokens::builtin::BuiltinType,
};

/// An index in one analyzed project's type table; not a layout or ABI identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeId(pub u32);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SemanticType {
    /// Internal reservation; never a successful semantic type.
    Pending,
    Builtin(BuiltinType),
    Pointer(TypeId),
    /// Qualifies exactly one pointer/array/slice layer, not its binding.
    Readonly(TypeId),
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
    /// Symbolic result of a type-returning call while checking its caller's body.
    /// Evaluation replaces it with an actual denoted type at a concrete call.
    TypeResult {
        function: DeclId,
        arguments: Vec<ComputationArgument>,
    },
    MemberResult {
        owner: TypeId,
        spelling: String,
    },
    IndexResult {
        owner: TypeId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ComputationArgument {
    Type(TypeId),
    Bool(bool),
    /// A bounded boolean recipe, with lexical binding identities preserved.
    Expression(Box<crate::frontend::ast::expression::Expression>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FunctionType {
    /// Eligibility for bounded compile-time type computation, not known identity.
    pub is_comp: bool,
    pub parameters: Vec<TypeId>,
    pub return_type: TypeId,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FieldType {
    pub spelling: String,
    pub ty: TypeId,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VariantType {
    pub spelling: String,
    pub payload: Option<TypeId>,
}

/// Owned by `AnalyzedProject`. Empty is a valid workspace, not proof of analysis.
/// Canonical types and binding facts. Layout and complete analysis are separate.
#[derive(Debug, Clone, Default)]
pub struct SemanticTypes {
    pub types: Vec<SemanticType>,
    pub declarations: HashMap<DeclId, TypeId>,
    pub locals: HashMap<LocalId, TypeId>,
    /// Value type/category, separate from the type denoted by a metatype value.
    pub bindings: HashMap<BindingId, ExpressionInfo>,
    pub type_values: HashMap<BindingId, TypeId>,
    pub variant_constructors: HashMap<(TypeId, u32), TypeId>,
    pub computation_results: HashMap<(DeclId, Vec<ComputationArgument>), TypeId>,
    /// Metatype parameters in source order, owned by the defining function.
    pub generic_parameters: HashMap<DeclId, Vec<LocalId>>,
    pub generic_instances: HashMap<
        crate::frontend::analyzer::generics::GenericInstanceKey,
        crate::frontend::analyzer::generics::GenericInstance,
    >,
    pub(crate) started: bool,
    pub(crate) complete: bool,
    interned: HashMap<SemanticType, TypeId>,
}

pub(crate) fn type_error(message: impl Into<String>) -> anyhow::Error {
    crate::diagnostics::CompilerError::new(crate::diagnostics::ErrorKind::Type, message.into())
        .into()
}

impl SemanticTypes {
    pub fn intern(&mut self, ty: SemanticType) -> TypeId {
        // Aggregates are nominal even when their shapes happen to coincide.
        if !matches!(
            ty,
            SemanticType::Struct(_) | SemanticType::Enum(_) | SemanticType::Pending
        ) {
            if let Some(id) = self.interned.get(&ty) {
                return *id;
            }
            // Public raw tables remain usable; do not assume all entries were interned.
            if let Some(index) = self.types.iter().position(|existing| *existing == ty) {
                let id = TypeId(index as u32);
                self.interned.insert(ty, id);
                return id;
            }
        }
        let id = TypeId(u32::try_from(self.types.len()).expect("semantic type ID space exhausted"));
        self.types.push(ty.clone());
        if !matches!(
            ty,
            SemanticType::Struct(_) | SemanticType::Enum(_) | SemanticType::Pending
        ) {
            self.interned.insert(ty, id);
        }
        id
    }
    pub fn builtin(&mut self, builtin: BuiltinType) -> TypeId {
        self.intern(SemanticType::Builtin(builtin))
    }
    pub fn get(&self, id: TypeId) -> anyhow::Result<&SemanticType> {
        let ty = self
            .types
            .get(id.0 as usize)
            .ok_or_else(|| type_error(format!("invalid type ID {id:?}")))?;
        if matches!(ty, SemanticType::Pending) {
            return Err(type_error(format!("incomplete type {id:?}")));
        }
        Ok(ty)
    }
    pub(crate) fn is_symbolic(&self, id: TypeId) -> anyhow::Result<bool> {
        Ok(matches!(
            self.get(self.canonical(id)?)?,
            SemanticType::Parameter(_)
                | SemanticType::TypeResult { .. }
                | SemanticType::MemberResult { .. }
                | SemanticType::IndexResult { .. }
        ))
    }
    pub fn is_dependent(&self, root: TypeId) -> anyhow::Result<bool> {
        let mut stack = vec![root];
        let mut seen = HashSet::new();
        while let Some(id) = stack.pop() {
            if !seen.insert(id) {
                continue;
            }
            match self.get(id)? {
                SemanticType::Parameter(_)
                | SemanticType::TypeResult { .. }
                | SemanticType::MemberResult { .. }
                | SemanticType::IndexResult { .. } => return Ok(true),
                SemanticType::Readonly(t) | SemanticType::Pointer(t) | SemanticType::Slice(t) => {
                    stack.push(*t)
                }
                SemanticType::Array { element, .. } => stack.push(*element),
                SemanticType::Tuple(ts) => stack.extend(ts),
                SemanticType::Function(f) => {
                    stack.extend(&f.parameters);
                    stack.push(f.return_type);
                }
                SemanticType::Struct(fs) => stack.extend(fs.iter().map(|f| f.ty)),
                SemanticType::Enum(vs) => stack.extend(vs.iter().filter_map(|v| v.payload)),
                SemanticType::Declared(_) => stack.push(self.canonical(id)?),
                _ => {}
            }
        }
        Ok(false)
    }
    pub fn is_opaque(&self, id: TypeId) -> anyhow::Result<bool> {
        Ok(matches!(
            self.get(id)?,
            SemanticType::Builtin(BuiltinType::Void)
        ))
    }
    pub(crate) fn bind(&mut self, binding: BindingId, info: ExpressionInfo) {
        match binding {
            BindingId::Declaration(id) => {
                self.declarations.insert(id, info.ty);
            }
            BindingId::Local(id) => {
                self.locals.insert(id, info.ty);
            }
        }
        if let Some(ty) = info.type_value {
            self.type_values.insert(binding, ty);
        }
        self.bindings.insert(binding, info);
    }
    pub fn validate_type(&self, root: TypeId) -> anyhow::Result<()> {
        let mut stack = vec![root];
        let mut seen = HashSet::new();
        while let Some(id) = stack.pop() {
            if !seen.insert(id) {
                continue;
            }
            match self.get(id)? {
                SemanticType::Readonly(t) => {
                    if !matches!(
                        self.get(self.canonical(*t)?)?,
                        SemanticType::Pointer(_)
                            | SemanticType::Array { .. }
                            | SemanticType::Slice(_)
                    ) {
                        return Err(type_error(
                            "const type qualifier requires a pointer, slice or array",
                        ));
                    }
                    stack.push(*t);
                }
                SemanticType::TypeResult { arguments, .. } => {
                    for arg in arguments {
                        if let ComputationArgument::Type(t) = arg {
                            stack.push(*t);
                        }
                    }
                }
                SemanticType::MemberResult { owner, .. } | SemanticType::IndexResult { owner } => {
                    stack.push(*owner)
                }
                SemanticType::Pointer(t) | SemanticType::Slice(t) => stack.push(*t),
                SemanticType::Array { element, .. } => stack.push(*element),
                SemanticType::Tuple(ts) => stack.extend(ts),
                SemanticType::Function(f) => {
                    stack.extend(&f.parameters);
                    stack.push(f.return_type);
                }
                SemanticType::Struct(fs) => stack.extend(fs.iter().map(|f| f.ty)),
                SemanticType::Enum(vs) => stack.extend(vs.iter().filter_map(|v| v.payload)),
                SemanticType::Declared(id) => stack.push(
                    *self
                        .type_values
                        .get(&BindingId::Declaration(*id))
                        .ok_or_else(|| type_error("incomplete declared type"))?,
                ),
                _ => {}
            }
        }
        Ok(())
    }
    /// Inspect one container layer without erasing its mutability from identity.
    pub(crate) fn container(&self, id: TypeId) -> anyhow::Result<(&SemanticType, bool)> {
        let id = self.canonical(id)?;
        match self.get(id)? {
            SemanticType::Readonly(inner) => Ok((self.get(self.canonical(*inner)?)?, true)),
            ty => Ok((ty, false)),
        }
    }
    pub(crate) fn canonical(&self, mut id: TypeId) -> anyhow::Result<TypeId> {
        let mut seen = HashSet::new();
        while let SemanticType::Declared(decl) = self.get(id)? {
            if !seen.insert(id) {
                return Err(type_error("type alias cycle"));
            }
            id = *self
                .type_values
                .get(&BindingId::Declaration(*decl))
                .ok_or_else(|| type_error("incomplete declared type"))?;
        }
        Ok(id)
    }
}
