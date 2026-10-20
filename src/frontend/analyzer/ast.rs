use super::{ResolveContext, TypeContext, at_span};
use crate::frontend::{
    analyzer::Analyzer,
    ast::{Ast, statement::StatementKind},
    namespace::name_error,
};

impl Ast {
    /// Resolve a file after project declaration collection and alias normalization.
    pub fn resolve_name(&mut self, context: &mut ResolveContext<'_>) -> anyhow::Result<()> {
        for statement in &mut self.statements {
            let result = match &mut statement.kind {
                StatementKind::FunctionDeclaration(fun) => fun.resolve_name(context),
                StatementKind::VariableDeclaration(var) => (|| {
                    if let Some(ty) = &mut var.ty {
                        ty.resolve_name(context)?;
                    }
                    if let Some(expr) = &mut var.expression {
                        Analyzer::resolve_name_expression(expr, context)?;
                    }
                    Analyzer::check_binding(var, var.expression.as_ref(), context)?;
                    Ok(())
                })(),
                StatementKind::MultiVariableDeclaration { bindings, values } => {
                    Analyzer::resolve_multi_bindings(bindings, values, context, true)
                }
                _ => Err(name_error("unexpected top-level statement")),
            };
            result.map_err(|e| at_span(e, statement.span))?;
        }
        Ok(())
    }

    pub fn resolve_type(&mut self, context: &mut TypeContext<'_>) -> anyhow::Result<()> {
        context.statements(&mut self.statements)
    }
}
