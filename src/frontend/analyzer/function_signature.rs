use crate::frontend::{ast::function_signature::FunctionSignature, semantic_type::TypeId};

use super::{ResolveContext, TypeContext, not_implemented};

impl FunctionSignature {
    /// Resolve signature types and bind parameters in the caller's function
    /// scope, without redeclaring the function's file-level identity.
    pub fn resolve_name(&mut self, _context: &mut ResolveContext<'_>) -> anyhow::Result<()> {
        not_implemented("function signature name resolution")
    }

    /// Resolve signatures before bodies so recursive calls can use them.
    pub fn resolve_type(&mut self, _context: &mut TypeContext<'_>) -> anyhow::Result<TypeId> {
        not_implemented("function signature type resolution")
    }
}
