use crate::frontend::ast::Ast;

use super::{ResolveContext, TypeContext, not_implemented};

impl Ast {
    // TODO: move functions that are implemented directly on analyzer.rs to here
    /// Resolve uses in a file whose declarations have already been collected.
    pub fn resolve_name(&mut self, _context: &mut ResolveContext<'_>) -> anyhow::Result<()> {
        not_implemented("file name resolution")
    }

    /// Enrich the same AST; IR lowering belongs after project analysis.
    pub fn resolve_type(&mut self, _context: &mut TypeContext<'_>) -> anyhow::Result<()> {
        not_implemented("file type resolution")
    }
}
