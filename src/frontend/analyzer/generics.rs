//! Generic specialization contracts use semantic identities, not string mangling
//! or cloned typed functions. Emission/naming of instances belongs to lowering.

use crate::frontend::{identifier::DeclId, semantic_type::TypeId};

use super::{TypeContext, not_implemented};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GenericInstanceKey {
    pub template: DeclId,
    pub arguments: Vec<TypeId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenericInstance {
    pub key: GenericInstanceKey,
    pub signature: TypeId,
}

/// Check type arguments and resolve an instance signature. Substitution,
/// canonicalization, recursion handling, and caching are all still unimplemented.
pub fn resolve_generic_instance(
    _key: &GenericInstanceKey,
    _context: &mut TypeContext<'_>,
) -> anyhow::Result<GenericInstance> {
    not_implemented("generic instance resolution")
}
