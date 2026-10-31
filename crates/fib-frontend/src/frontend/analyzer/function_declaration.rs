use crate::frontend::{ast::function_declaration::FunctionDeclaration, semantic_type::TypeId};

use super::{ResolveContext, TypeContext};

impl FunctionDeclaration {
    /// The declaration is predeclared at file level. Parameters, generic
    /// parameters, and the body share a function lexical environment; locals
    /// must not leak to another function or capture an enclosing function.
    pub fn resolve_name(&mut self, context: &mut ResolveContext<'_>) -> anyhow::Result<()> {
        use crate::frontend::{analyzer::Analyzer, scope_kind::ScopeKind};
        context.scoped(ScopeKind::Function, |context| {
            Analyzer::resolve_name_signature(&mut self.signature, context)?;
            if let Some(body) = &mut self.body {
                for statement in &mut body.statements {
                    Analyzer::resolve_name_statement(statement, context)?;
                }
            }
            Ok(())
        })
    }

    /// Check the body against its already-resolved signature, including returns.
    pub fn resolve_type(
        &mut self,
        context: &mut TypeContext<'_>,
        signature: TypeId,
    ) -> anyhow::Result<()> {
        use crate::frontend::semantic_type::{SemanticType, type_error};
        let resolved = context.signature(&mut self.signature)?;
        super::types::check_assignable(signature, resolved, context.types)?;
        let SemanticType::Function(function) = context.types.get(signature)? else {
            return Err(type_error("expected a function signature"));
        };
        if self.signature.is_comp {
            if self.is_extern || self.body.is_none() {
                return Err(type_error(
                    "comp function requires a non-extern function body",
                ));
            }
            if matches!(
                context.types.get(function.return_type)?,
                SemanticType::Builtin(crate::frontend::tokens::builtin::BuiltinType::MetaType)
            ) {
                super::evaluation::validate_comp_body(&self.body.as_ref().unwrap().statements)?;
            }
        }
        if self.body.is_none() && !self.is_extern {
            return Err(type_error("bodyless functions require callconv \"C\""));
        }
        if self.is_extern
            != (self.signature.calling_convention
                == crate::frontend::semantic_type::CallConvention::C)
        {
            return Err(type_error(
                "function calling convention annotation mismatch",
            ));
        }
        let previous_comp =
            std::mem::replace(&mut context.state.comp_function, self.signature.is_comp);
        let previous = context.state.return_type.replace(function.return_type);
        let previous_loop = std::mem::take(&mut context.state.loop_depth);
        let previous_defer = std::mem::take(&mut context.state.in_defer);
        let result = if let Some(body) = &mut self.body {
            context.statements(&mut body.statements)
        } else {
            Ok(())
        };
        context.state.return_type = previous;
        context.state.comp_function = previous_comp;
        context.state.loop_depth = previous_loop;
        context.state.in_defer = previous_defer;
        result
    }
}

/// Type-owned functions have no closure context. Check reads and assignment
/// targets alike, rather than relying on definite initialization to catch reads.
pub(super) fn validate_member_captures(
    function: &FunctionDeclaration,
    types: &crate::frontend::semantic_type::SemanticTypes,
) -> anyhow::Result<()> {
    use crate::{
        diagnostics::Span,
        frontend::{
            analysis_annotation::{BindingAnnotation, BindingId, ValueCategory},
            ast::qualified_name::QualifiedIdentifier,
            identifier::ResolvedName,
            semantic_type::type_error,
        },
    };
    use std::collections::HashSet;
    let Some(body) = &function.body else {
        return Ok(());
    };
    struct Owned(HashSet<BindingId>);
    impl super::walk::Visitor for Owned {
        fn binding(
            &mut self,
            _: &str,
            slot: &mut BindingAnnotation,
            _: Span,
        ) -> anyhow::Result<()> {
            if let Some(id) = slot.binding {
                self.0.insert(id);
            }
            Ok(())
        }
    }
    let mut owned = Owned(
        function
            .signature
            .parameters
            .iter()
            .filter_map(|p| p.annotation.binding)
            .collect(),
    );
    let mut body = body.statements.clone();
    super::walk::statements(&mut owned, &mut body, false)?;
    struct Captures<'a>(
        &'a HashSet<BindingId>,
        &'a crate::frontend::semantic_type::SemanticTypes,
    );
    impl super::walk::Visitor for Captures<'_> {
        fn name(&mut self, name: &mut QualifiedIdentifier) -> anyhow::Result<()> {
            if let Some(ResolvedName::Local(id)) = name.annotation.resolved {
                let id = BindingId::Local(id);
                if !self.0.contains(&id)
                    && !self.1.type_values.contains_key(&id)
                    && self
                        .1
                        .bindings
                        .get(&id)
                        .is_none_or(|info| info.category != ValueCategory::Value)
                {
                    let error = type_error(
                        "comp member function cannot capture an enclosing runtime local or parameter",
                    );
                    return Err(if let Some(span) = name.annotation.source_span {
                        super::at_span(error, span.into())
                    } else {
                        error
                    });
                }
            }
            Ok(())
        }
    }
    super::walk::statements(&mut Captures(&owned.0, types), &mut body, false)
}
