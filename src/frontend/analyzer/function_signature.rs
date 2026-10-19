use crate::frontend::{
    analyzer::Analyzer, ast::function_signature::FunctionSignature, semantic_type::TypeId,
};

use super::{ResolveContext, TypeContext, not_implemented};

impl Analyzer {
    /// Resolve signature types and bind parameters in the caller's function
    /// scope, without redeclaring the function's file-level identity.
    // TODO(review): Allocate parameter identities, annotate their binding sites,
    // and register them in the function scope before resolving the body. This
    // stub currently makes resolve_names fail for every function with a body,
    // including functions without parameters.
    pub fn resolve_name_signature(
        _fun_sig: &mut FunctionSignature,
        _context: &mut ResolveContext<'_>,
    ) -> anyhow::Result<()> {
        not_implemented("function signature name resolution")
    }

    /// Resolve signatures before bodies so recursive calls can use them.
    pub fn resolve_types_signature(
        &mut self,
        _context: &mut TypeContext<'_>,
    ) -> anyhow::Result<TypeId> {
        not_implemented("function signature type resolution")
    }
}
