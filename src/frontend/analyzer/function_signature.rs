use crate::frontend::{
    analyzer::Analyzer, ast::function_signature::FunctionSignature, semantic_type::TypeId,
};

use super::{ResolveContext, TypeContext, not_implemented};

impl Analyzer {
    /// Resolve signature types and bind parameters in the caller's function
    /// scope, without redeclaring the function's file-level identity.
    pub fn resolve_name_signature(
        fun_sig: &mut FunctionSignature,
        context: &mut ResolveContext<'_>,
    ) -> anyhow::Result<()> {
        use crate::frontend::{
            analysis_annotation::BindingId, namespace::name_error, symbol_table::LocalBinding,
        };
        if !fun_sig.generic_parameters.is_empty() {
            return Err(name_error("generic declarations are not supported yet"));
        }
        // Parameters are values; none participates in lookup of signature types.
        for parameter in &mut fun_sig.parameters {
            parameter.parameter_type.resolve_name(context)?;
        }
        fun_sig.return_type.resolve_name(context)?;
        for parameter in &mut fun_sig.parameters {
            Self::check_local_name(&parameter.parameter_identifier, false, context)
                .map_err(|e| super::at_span(e, parameter.parameter_type.span))?;
            let id = context.local_ids.allocate()?;
            context.lexical.insert_binding(
                parameter.parameter_identifier.clone(),
                LocalBinding::Local(id),
            )?;
            parameter.annotation.binding = Some(BindingId::Local(id));
        }
        Ok(())
    }

    /// Resolve signatures before bodies so recursive calls can use them.
    pub fn resolve_types_signature(
        &mut self,
        _context: &mut TypeContext<'_>,
    ) -> anyhow::Result<TypeId> {
        not_implemented("function signature type resolution")
    }
}
