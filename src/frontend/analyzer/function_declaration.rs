use crate::frontend::{ast::function_declaration::FunctionDeclaration, semantic_type::TypeId};

use super::{ResolveContext, TypeContext, not_implemented};

impl FunctionDeclaration {
    /// The declaration is predeclared at file level. Parameters, generic
    /// parameters, and the body share a function lexical environment; locals
    /// must not leak to another function or capture an enclosing function.
    pub fn resolve_name(&mut self, _context: &mut ResolveContext<'_>) -> anyhow::Result<()> {
        not_implemented("function name resolution")
    }

    /// Check the body against its already-resolved signature, including returns.
    pub fn resolve_type(
        &mut self,
        _context: &mut TypeContext<'_>,
        _signature: TypeId,
    ) -> anyhow::Result<()> {
        not_implemented("function body type resolution")
    }
}
