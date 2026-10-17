use crate::frontend::{ast::switch::SwitchArm, semantic_type::TypeId};

use super::{ResolveContext, TypeContext, not_implemented};

impl SwitchArm {
    /// Payload bindings are locals in an arm scope; variant names are members.
    pub fn resolve_name(&mut self, _context: &mut ResolveContext<'_>) -> anyhow::Result<()> {
        not_implemented("switch arm name resolution")
    }

    pub fn resolve_type(
        &mut self,
        _context: &mut TypeContext<'_>,
        _subject: TypeId,
    ) -> anyhow::Result<()> {
        not_implemented("switch arm type resolution")
    }
}
