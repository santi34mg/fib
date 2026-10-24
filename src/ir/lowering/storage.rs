//! Storage classification before SSA construction and alias-visible reads.
use super::*;

pub(super) fn storage_bindings(
    nodes: &[Statement],
    source: &Source<'_>,
) -> anyhow::Result<std::collections::BTreeSet<LocalId>> {
    use std::collections::BTreeSet;
    fn root(expr: &Expression) -> Option<LocalId> {
        local(expr).or_else(|| match &expr.kind {
            E::ExprAcess { expr, .. } => root(expr),
            _ => None,
        })
    }
    fn expression(expr: &Expression, stored: &mut BTreeSet<LocalId>) {
        match &expr.kind {
            E::UnaryExpression(u) => {
                if u.operator == Operator::AddressOf
                    && let Some(id) = root(&u.expression)
                {
                    stored.insert(id);
                }
                expression(&u.expression, stored);
            }
            E::BinaryExpresison(b) => {
                expression(&b.left, stored);
                expression(&b.right, stored);
            }
            E::Call { args, .. } => {
                for arg in args {
                    expression(arg, stored);
                }
            }
            E::ExprAcess { expr, .. } | E::Cast { expr, .. } => expression(expr, stored),
            E::StructConstruct { fields, .. } => {
                for field in fields {
                    expression(&field.expression, stored);
                }
            }
            _ => {}
        }
    }
    fn statements(
        nodes: &[Statement],
        source: &Source<'_>,
        stored: &mut BTreeSet<LocalId>,
    ) -> anyhow::Result<()> {
        for node in nodes {
            match &node.kind {
                S::VariableDeclaration(v) if !v.is_comp => {
                    if let Some(BindingId::Local(id)) = v.annotation.binding
                        && (v.expression.is_none() || source.is_aggregate_local(id)?)
                    {
                        stored.insert(id);
                    }
                    if let Some(expr) = &v.expression {
                        expression(expr, stored);
                    }
                }
                S::Assignment { left, right } => {
                    expression(left, stored);
                    expression(right, stored);
                }
                S::ExpressionStatement(e) => expression(e, stored),
                S::Return(Some(values)) => {
                    for e in values {
                        expression(e, stored);
                    }
                }
                S::While { condition, body } => {
                    if let Some(e) = condition {
                        expression(e, stored);
                    }
                    statements(body, source, stored)?;
                }
                S::Switch { subject, arms } => {
                    expression(subject, stored);
                    for arm in arms {
                        if let Pattern::EnumVariant {
                            binding: Some(binding),
                            ..
                        } = &arm.pattern
                            && let Some(BindingId::Local(id)) = binding.annotation.binding
                        {
                            stored.insert(id);
                        }
                        statements(&arm.body, source, stored)?;
                    }
                }
                S::If {
                    condition,
                    then_branch,
                    else_branch,
                } => {
                    expression(condition, stored);
                    statements(then_branch, source, stored)?;
                    if let Some(branch) = else_branch {
                        statements(branch, source, stored)?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
    let mut stored = BTreeSet::new();
    statements(nodes, source, &mut stored)?;
    Ok(stored)
}
