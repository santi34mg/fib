use crate::frontend::{
    analyzer::Analyzer, ast::function_signature::FunctionSignature, semantic_type::TypeId,
};

use super::{ResolveContext, TypeContext};

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
        // Only metatype parameters participate in signature type lookup.
        // Runtime parameters retain the existing signature-before-value scope.
        let mut names = std::collections::HashSet::new();
        for parameter in &mut fun_sig.parameters {
            Self::check_local_name(&parameter.parameter_identifier, false, context)
                .map_err(|e| super::at_span(e, parameter.parameter_type.span))?;
            if !names.insert(parameter.parameter_identifier.clone()) {
                return Err(super::at_span(
                    name_error(format!(
                        "duplicate lexical identifier '{}'",
                        parameter.parameter_identifier
                    )),
                    parameter.parameter_type.span,
                ));
            }
            let id = context.local_ids.allocate()?;
            parameter.annotation.binding = Some(BindingId::Local(id));
            if context
                .symbols
                .is_metatype_syntax(context.file_id, &parameter.parameter_type)
            {
                context.lexical.insert_binding(
                    parameter.parameter_identifier.clone(),
                    LocalBinding::Local(id),
                )?;
            }
        }
        for parameter in &mut fun_sig.parameters {
            parameter.parameter_type.resolve_name(context)?;
        }
        fun_sig.return_type.resolve_name(context)?;
        for parameter in &fun_sig.parameters {
            if !context
                .symbols
                .is_metatype_syntax(context.file_id, &parameter.parameter_type)
            {
                let Some(BindingId::Local(id)) = parameter.annotation.binding else {
                    unreachable!()
                };
                context.lexical.insert_binding(
                    parameter.parameter_identifier.clone(),
                    LocalBinding::Local(id),
                )?;
            }
        }
        Ok(())
    }

    /// Resolve signatures before bodies so recursive calls can use them.
    pub fn resolve_types_signature(
        signature: &mut FunctionSignature,
        context: &mut TypeContext<'_>,
    ) -> anyhow::Result<TypeId> {
        context.signature(signature)
    }
}
