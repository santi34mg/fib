use super::Analyzable;
use std::collections::HashMap;

use crate::frontend::tokens::symbol::Symbol;
use crate::frontend::typed_ast::ty::Ty;
use crate::frontend::typed_ast::typed_statement::TypedStatement;
use crate::frontend::typed_ast::{TypedBinding, TypedExpr, TypedFunction, TypedSymbol};
use crate::frontend::{
    ast::expression::{Expression, ExpressionKind},
    typed_ast::symbol_table::SymbolTable,
};

use super::AnalysisError;

pub fn validate_assignment_target(
    target: &Expression,
    symbol_table: &SymbolTable,
) -> Result<(), AnalysisError> {
    match &target.kind {
        ExpressionKind::Symbol(symbol) => {
            if let Some(TypedSymbol::Binding(binding)) = symbol_table.lookup(&symbol.id)
                && !binding.mutable
            {
                return Err(format!("cannot assign to constant '{}'", symbol).into());
            }
            Ok(())
        }
        ExpressionKind::StructFieldAccess { object, .. } => {
            if let ExpressionKind::Symbol(symbol) = &object.kind
                && let Some(TypedSymbol::Binding(binding)) = symbol_table.lookup(&symbol.id)
                && !binding.mutable
            {
                return Err(format!("cannot assign to field of constant '{}'", symbol).into());
            }
            Ok(())
        }
        ExpressionKind::Dereference(_) | ExpressionKind::IndexAccess { .. } => Ok(()),
        _ => Err("invalid multi-assignment target".to_string().into()),
    }
}

/// Shared arity rule for `a, b = ...` (multi-assignment) and `q, r := ...`
/// (multi-variable declaration): either one value per target, or a single
/// tuple-typed value with one element per target. Returns the per-target
/// types on success, `None` on arity mismatch (callers attach their own
/// `multi-assignment` / `multi-variable declaration` wording).
fn resolve_multi_value_types<'t>(target_count: usize, values: &[TypedExpr]) -> Option<Vec<Ty<'t>>> {
    if values.len() == target_count {
        return Some(
            values
                .iter()
                .map(|value| value.inferred_type.clone())
                .collect(),
        );
    }
    if values.len() == 1
        && let Ty::Tuple { elements } = &values[0].inferred_type
        && elements.len() == target_count
    {
        return Some(elements.clone());
    }
    None
}

pub fn validate_multi_assignment_shape(
    targets: &[TypedExpr],
    values: &[TypedExpr],
) -> Result<(), AnalysisError> {
    match resolve_multi_value_types(targets.len(), values) {
        Some(_) => Ok(()),
        None => Err(format!(
            "multi-assignment arity mismatch: {} target(s), {} value expression(s)",
            targets.len(),
            values.len()
        )
        .into()),
    }
}

pub(super) fn infer_multi_binding_types<'t>(
    identifiers: &[Symbol],
    values: &[TypedExpr],
) -> Result<Vec<Ty<'t>>, AnalysisError> {
    match resolve_multi_value_types(identifiers.len(), values) {
        Some(binding_types) => Ok(binding_types),
        None => Err(format!(
            "multi-variable declaration arity mismatch: {} identifier(s), {} value expression(s)",
            identifiers.len(),
            values.len()
        )
        .into()),
    }
}

pub fn multi_var_decl_to_typed<'t>(
    identifiers: &'t [Symbol],
    values: &'t [Expression],
    symbol_table: &mut SymbolTable,
    generic_cache: &mut HashMap<&'t str, TypedFunction>,
) -> anyhow::Result<TypedStatement<'t>> {
    let mut typed_values = Vec::new();
    for value in values {
        typed_values.push(value.analyze(symbol_table, generic_cache)?);
    }

    let binding_types = infer_multi_binding_types(&identifiers, &typed_values)?;
    let mut bindings = Vec::new();
    for (identifier, ty) in identifiers.into_iter().zip(binding_types) {
        let binding = TypedBinding {
            symbol: identifier.clone(),
            ty,
            init: None,
            mutable: true,
        };
        symbol_table.insert(identifier.id, TypedSymbol::Binding(binding.clone()));
        bindings.push(binding);
    }

    Ok(TypedStatement::MultiBinding {
        bindings: &bindings,
        values: &typed_values,
    })
}
