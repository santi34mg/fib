use crate::frontend::{ast::switch::SwitchArm, semantic_type::TypeId};

use super::{ResolveContext, TypeContext, not_implemented};

impl SwitchArm {
    /// Payload bindings are locals in an arm scope; variant names are members.
    pub fn resolve_name(&mut self, context: &mut ResolveContext<'_>) -> anyhow::Result<()> {
        use crate::frontend::{
            analysis_annotation::BindingId, analyzer::Analyzer, ast::pattern::Pattern,
            scope_kind::ScopeKind, symbol_table::LocalBinding,
        };
        context.scoped(ScopeKind::Block, |context| {
            if let Pattern::EnumVariant {
                binding: Some(binding),
                ..
            } = &mut self.pattern
            {
                Analyzer::check_local_name(&binding.identifier, false, context)?;
                let id = context.local_ids.allocate()?;
                binding.annotation.binding = Some(BindingId::Local(id));
                context
                    .lexical
                    .insert_binding(binding.identifier.clone(), LocalBinding::Local(id))?;
            }
            for statement in &mut self.body {
                Analyzer::resolve_name_statement(statement, context)?;
            }
            Ok(())
        })
    }

    pub fn resolve_type(
        &mut self,
        _context: &mut TypeContext<'_>,
        _subject: TypeId,
    ) -> anyhow::Result<()> {
        not_implemented("switch arm type resolution")
    }
}
