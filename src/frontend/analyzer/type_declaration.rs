use crate::frontend::{ast::type_declaration::TypeDeclaration, semantic_type::TypeId};

use super::{ResolveContext, TypeContext, not_implemented};

impl TypeDeclaration {
    /// Resolve the definition of a predeclared type/compile-time binding.
    pub fn resolve_name(&mut self, _context: &mut ResolveContext<'_>) -> anyhow::Result<()> {
        not_implemented("type declaration name resolution")
    }

    pub fn resolve_type(&mut self, _context: &mut TypeContext<'_>) -> anyhow::Result<TypeId> {
        not_implemented("type declaration resolution")
    }
}
