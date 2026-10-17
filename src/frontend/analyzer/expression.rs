use crate::frontend::{
    analysis_annotation::{ExpressionInfo, MemberInfo},
    ast::expression::Expression,
    semantic_type::{SemanticTypes, TypeId},
};

use super::{ResolveContext, TypeContext, not_implemented};

impl Expression {
    /// Resolve name uses, not binding sites. For member access resolve only the
    /// receiver here; never insert a field name or rewrite its source spelling.
    pub fn resolve_name(&mut self, _context: &mut ResolveContext<'_>) -> anyhow::Result<()> {
        not_implemented("expression name resolution")
    }

    /// Infer/check a value and annotate its type and place category in situ.
    pub fn resolve_type(
        &mut self,
        _context: &mut TypeContext<'_>,
        _expected: Option<TypeId>,
    ) -> anyhow::Result<ExpressionInfo> {
        not_implemented("expression type resolution")
    }
}

/// Used for selection and initializer-field validation after the owner type is
/// known. Deliberately has no mutable lexical table or source spelling access.
pub fn resolve_member(
    _owner: TypeId,
    _spelling: &str,
    _types: &SemanticTypes,
) -> anyhow::Result<MemberInfo> {
    not_implemented("type member resolution")
}
