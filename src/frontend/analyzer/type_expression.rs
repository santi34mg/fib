use crate::frontend::{ast::type_expression::TypeExpression, semantic_type::TypeId};

use super::{ResolveContext, TypeContext, not_implemented};

impl TypeExpression {
    /// Resolve referenced types without registering field or variant spellings
    /// in lexical scopes; member definitions belong to the semantic owner type.
    pub fn resolve_name(&mut self, _context: &mut ResolveContext<'_>) -> anyhow::Result<()> {
        not_implemented("type expression name resolution")
    }

    pub fn resolve_type(&mut self, _context: &mut TypeContext<'_>) -> anyhow::Result<TypeId> {
        not_implemented("type expression resolution")
    }
}
