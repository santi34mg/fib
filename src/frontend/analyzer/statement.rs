use crate::frontend::{
    analysis_annotation::ExpressionInfo,
    analyzer::Analyzer,
    ast::statement::{Statement, StatementKind},
    semantic_type::{SemanticTypes, TypeId},
};

use super::{ResolveContext, TypeContext};

impl Analyzer {
    /// Manage block lifetimes, declaration ordering, pattern bindings, and the
    /// separate control-flow label namespace. Do not insert names at use sites.
    pub fn resolve_name_statement(
        statement: &mut Statement,
        context: &mut ResolveContext<'_>,
    ) -> anyhow::Result<()> {
        Self::resolve_name_statement_inner(statement, context)
            .map_err(|e| super::at_span(e, statement.span))
    }

    fn resolve_name_statement_inner(
        statement: &mut Statement,
        context: &mut ResolveContext<'_>,
    ) -> anyhow::Result<()> {
        use crate::frontend::{namespace::name_error, scope_kind::ScopeKind};
        fn block(
            statements: &mut [Statement],
            context: &mut ResolveContext<'_>,
        ) -> anyhow::Result<()> {
            context.scoped(ScopeKind::Block, |context| {
                for statement in statements {
                    Analyzer::resolve_name_statement(statement, context)?;
                }
                Ok(())
            })
        }
        match &mut statement.kind {
            StatementKind::FunctionDeclaration(_) => {
                return Err(name_error("nested functions are not supported yet"));
            }
            StatementKind::VariableDeclaration(var) => {
                Self::resolve_name_variable_declaration(var, context)?
            }
            StatementKind::MultiVariableDeclaration { bindings, values } => {
                Self::resolve_multi_bindings(bindings, values, context, false)?
            }
            StatementKind::ExpressionStatement(expr) => {
                Self::resolve_name_expression(expr, context)?
            }
            StatementKind::Assignment { left, right } => {
                Self::resolve_name_expression(left, context)?;
                Self::resolve_name_expression(right, context)?;
            }
            StatementKind::DerefAssign { pointer, expr } => {
                Self::resolve_name_expression(pointer, context)?;
                Self::resolve_name_expression(expr, context)?;
            }
            StatementKind::IndexAssign {
                object,
                index,
                expr,
            } => {
                Self::resolve_name_expression(object, context)?;
                Self::resolve_name_expression(index, context)?;
                Self::resolve_name_expression(expr, context)?;
            }
            StatementKind::Return(values) => {
                if let Some(values) = values {
                    for value in values {
                        Self::resolve_name_expression(value, context)?;
                    }
                }
            }
            StatementKind::MultiAssignment { targets, values } => {
                for expr in targets.iter_mut().chain(values) {
                    Self::resolve_name_expression(expr, context)?;
                }
            }
            StatementKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                Self::resolve_name_expression(condition, context)?;
                block(then_branch, context)?;
                if let Some(branch) = else_branch {
                    block(branch, context)?;
                }
            }
            StatementKind::While { condition, body } => {
                if let Some(condition) = condition {
                    Self::resolve_name_expression(condition, context)?;
                }
                block(body, context)?;
            }
            StatementKind::Defer(statement) => Self::resolve_name_statement(statement, context)?,
            StatementKind::Switch { subject, arms } => {
                Self::resolve_name_expression(subject, context)?;
                for arm in arms {
                    arm.resolve_name(context)?;
                }
            }
            StatementKind::Break | StatementKind::Continue => {}
        }
        Ok(())
    }

    pub fn resolve_type_statement(
        statement: &mut Statement,
        context: &mut TypeContext<'_>,
    ) -> anyhow::Result<()> {
        context.statement(statement)
    }
}

/// Assignment legality uses the resolved place category, not token identity.
pub fn validate_assignment_target(target: &ExpressionInfo) -> anyhow::Result<()> {
    match target.category {
        crate::frontend::analysis_annotation::ValueCategory::MutablePlace => Ok(()),
        crate::frontend::analysis_annotation::ValueCategory::ImmutablePlace => Err(
            crate::frontend::semantic_type::type_error("assignment to immutable place"),
        ),
        _ => Err(crate::frontend::semantic_type::type_error(
            "assignment requires a mutable place",
        )),
    }
}

/// Shared contract for multi-assignment and multi-binding: check arity and
/// tuple unpacking, returning one semantic type per destination.
pub fn resolve_multi_value_types(
    target_count: usize,
    values: &[TypeId],
    types: &SemanticTypes,
) -> anyhow::Result<Vec<TypeId>> {
    for ty in values {
        types.validate_type(*ty)?;
    }
    if target_count == values.len() {
        return Ok(values.to_vec());
    }
    if values.len() == 1
        && target_count > 1
        && let crate::frontend::semantic_type::SemanticType::Tuple(elements) =
            types.get(values[0])?
        && elements.len() == target_count
    {
        return Ok(elements.clone());
    }
    Err(crate::frontend::semantic_type::type_error(
        "multi-value arity mismatch",
    ))
}
