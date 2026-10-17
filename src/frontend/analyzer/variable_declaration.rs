use crate::frontend::ast::variable_declaration::VariableDeclaration;

use super::{ResolveContext, TypeContext, not_implemented};

impl VariableDeclaration {
    /// Distinguish file declarations from new LocalId bindings and static comp
    /// aliases. A const initializer creates a value, never an alias identity.
    pub fn resolve_name(&mut self, _context: &mut ResolveContext<'_>) -> anyhow::Result<()> {
        not_implemented("variable declaration name resolution")
    }

    pub fn resolve_type(&mut self, _context: &mut TypeContext<'_>) -> anyhow::Result<()> {
        not_implemented("variable declaration type resolution")
    }
}
