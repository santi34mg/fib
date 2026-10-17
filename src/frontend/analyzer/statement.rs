use crate::frontend::{
    analysis_annotation::ExpressionInfo,
    ast::statement::Statement,
    semantic_type::{SemanticTypes, TypeId},
};

use super::{ResolveContext, TypeContext, not_implemented};

impl Statement {
    /// Manage block lifetimes, declaration ordering, pattern bindings, and the
    /// separate control-flow label namespace. Do not insert names at use sites.
    pub fn resolve_name(&mut self, _context: &mut ResolveContext<'_>) -> anyhow::Result<()> {
        not_implemented("statement name resolution")
    }

    pub fn resolve_type(&mut self, _context: &mut TypeContext<'_>) -> anyhow::Result<()> {
        not_implemented("statement type resolution")
    }
}

/// Assignment legality uses the resolved place category, not token identity.
pub fn validate_assignment_target(_target: &ExpressionInfo) -> anyhow::Result<()> {
    not_implemented("assignment target validation")
}

/// Shared contract for multi-assignment and multi-binding: check arity and
/// tuple unpacking, returning one semantic type per destination.
pub fn resolve_multi_value_types(
    _target_count: usize,
    _values: &[TypeId],
    _types: &SemanticTypes,
) -> anyhow::Result<Vec<TypeId>> {
    not_implemented("multi-value type resolution")
}
