use anyhow::bail;

use crate::frontend::{
    analyzer::Analyzer, ast::variable_declaration::VariableDeclaration, symbol_table::LocalBinding,
};

use super::{ResolveContext, TypeContext, not_implemented};

impl Analyzer {
    /// Distinguish file declarations from new LocalId bindings and static comp
    /// aliases. A const initializer creates a value, never an alias identity.
    pub fn resolve_name_variable_declaration(
        variable_declaration: &mut VariableDeclaration,
        context: &mut ResolveContext<'_>,
    ) -> anyhow::Result<()> {
        let None = context
            .lexical
            .resolve_lexical(&variable_declaration.identifier)
        else {
            bail!("found duplicate identifier")
        };
        let local_id = context.local_ids.allocate()?;
        context.lexical.insert_binding(
            variable_declaration.identifier.clone(),
            LocalBinding::Local(local_id),
        )?;
        if let Some(ref mut expression) = variable_declaration.expression {
            Self::resolve_name_expression(expression, context)?;
        }
        Ok(())
    }

    pub fn resolve_type_variable_declaration(
        &mut self,
        _context: &mut TypeContext<'_>,
    ) -> anyhow::Result<()> {
        not_implemented("variable declaration type resolution")
    }
}
