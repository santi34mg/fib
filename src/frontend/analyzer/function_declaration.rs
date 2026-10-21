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
            if !matches!(
                context.types.get(function.return_type)?,
                SemanticType::Builtin(crate::frontend::tokens::builtin::BuiltinType::MetaType)
            ) {
                return Err(type_error("bounded comp function must return type"));
            }
            super::evaluation::validate_comp_body(&self.body.as_ref().unwrap().statements)?;
        }
        let previous_comp =
            std::mem::replace(&mut context.state.comp_function, self.signature.is_comp);
        let previous = context.state.return_type.replace(function.return_type);
        let result = if let Some(body) = &mut self.body {
            context.statements(&mut body.statements)
        } else {
            Ok(())
        };
        context.state.return_type = previous;
        context.state.comp_function = previous_comp;
        result
    }
}
