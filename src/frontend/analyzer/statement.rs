use crate::frontend::{
    analysis_annotation::ExpressionInfo,
    analyzer::Analyzer,
    ast::statement::{Statement, StatementKind},
    semantic_type::{SemanticTypes, TypeId},
};

use super::{ResolveContext, TypeContext, not_implemented};

impl Analyzer {
    /// Manage block lifetimes, declaration ordering, pattern bindings, and the
    /// separate control-flow label namespace. Do not insert names at use sites.
    pub fn resolve_name_statement(
        statement: &mut Statement,
        context: &mut ResolveContext,
    ) -> anyhow::Result<()> {
        match statement.kind {
            StatementKind::FunctionDeclaration(ref mut function_declaration) => todo!(),
            StatementKind::VariableDeclaration(ref mut variable_declaration) => {
                Self::resolve_name_variable_declaration(variable_declaration, context)?;
            }
            StatementKind::MultiVariableDeclaration {
                ref mut bindings,
                ref mut values,
            } => todo!(),
            StatementKind::ExpressionStatement(ref mut expression) => {
                Self::resolve_name_expression(expression, context)?;
            }
            StatementKind::Assignment {
                ref mut left,
                ref mut right,
            } => {
                todo!()
            }
            StatementKind::DerefAssign {
                ref mut pointer,
                ref mut expr,
            } => todo!(),
            StatementKind::IndexAssign {
                ref mut object,
                ref mut index,
                ref mut expr,
            } => todo!(),
            StatementKind::Return(ref mut expressions) => todo!(),
            StatementKind::MultiAssignment {
                ref mut targets,
                ref mut values,
            } => todo!(),
            StatementKind::If {
                ref mut condition,
                ref mut then_branch,
                ref mut else_branch,
            } => todo!(),
            StatementKind::While {
                ref mut condition,
                ref mut body,
            } => todo!(),
            StatementKind::Break => todo!(),
            StatementKind::Continue => todo!(),
            StatementKind::Defer(ref mut statement) => todo!(),
            StatementKind::Switch {
                ref mut subject,
                ref mut arms,
            } => todo!(),
        }
        Ok(())
    }

    pub fn resolve_type_statement(_context: &mut TypeContext<'_>) -> anyhow::Result<()> {
        not_implemented("statement type resolution")
    }
}

/// Assignment legality uses the resolved place category, not token identity.
pub fn validate_assignment_target(_target: &ExpressionInfo) -> anyhow::Result<()> {
    not_implemented("assignment target validation")
}

/// Shared contract for multi-assignment and multi-binding: check arity and
/// tuple unpacking, returning one semantic type per destination.
pub fn resolve_multi_value_types(
    _target_count: usize,
    _values: &[TypeId],
    _types: &SemanticTypes,
) -> anyhow::Result<Vec<TypeId>> {
    not_implemented("multi-value type resolution")
}
