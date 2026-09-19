use std::collections::HashMap;

use crate::frontend::ast::{
    expression::{Expression as PExpr, ExpressionKind as PExprKind},
    pattern::Pattern,
    statement::Statement,
    statement::StatementKind,
    type_expression::TypeExpressionKind,
    variable_declaration::VariableDeclaration,
};
use crate::frontend::identifier::Identifier;
use crate::frontend::typed_ast::{
    ScopeKind, SymbolTable, Ty, TypedBinding, TypedExpr, TypedExprKind, TypedFunction, TypedIf,
    TypedPattern, TypedReturn, TypedStatement, TypedSwitchArm, TypedSymbol,
};

use super::AnalysisError;
use super::expressions::{check_assignable, coerce_to, expr_to_typed, require_integer_index};
use super::types::{map_type, resolve_struct_fields, resolve_type_alias};

pub(super) fn stmt_to_typed(
    stmt: Statement,
    current_scope: &mut SymbolTable,
    generic_cache: &mut HashMap<String, TypedFunction>,
) -> Result<TypedStatement, AnalysisError> {
    stmt_to_typed_at(stmt, current_scope, generic_cache, 0)
}

/// Depth-aware core: `loop_depth` counts lexically enclosing `for` loops so
/// `break`/`continue` can be rejected outside any loop. Nested body statements
/// recurse through this (preserving depth) instead of `stmt_to_typed`, which
/// always starts at depth 0.
fn stmt_to_typed_at(
    stmt: Statement,
    current_scope: &mut SymbolTable,
    generic_cache: &mut HashMap<String, TypedFunction>,
    loop_depth: usize,
) -> Result<TypedStatement, AnalysisError> {
    let span = stmt.span;
    stmt_to_typed_inner(stmt.kind, current_scope, generic_cache, loop_depth)
        .map_err(|e| e.with_span_fallback(span))
}

pub(super) fn stmt_to_typed_inner(
    stmt: StatementKind,
    current_scope: &mut SymbolTable,
    generic_cache: &mut HashMap<String, TypedFunction>,
    loop_depth: usize,
) -> Result<TypedStatement, AnalysisError> {
    match stmt {
        StatementKind::VariableDeclaration(variable_declaration) => Ok(TypedStatement::Binding(
            var_decl_to_typed(variable_declaration, current_scope, generic_cache)?,
        )),
        StatementKind::Assignment { identifier, expr } => {
            let target_ty = match current_scope.lookup(&identifier) {
                Some(TypedSymbol::Binding(binding)) => {
                    if !binding.mutable {
                        return Err(AnalysisError::from(format!(
                            "cannot assign to constant '{}'",
                            identifier
                        ))
                        .with_hint("declare the binding with `var` to make it mutable"));
                    }
                    binding.ty.clone()
                }
                Some(_) => {
                    return Err(format!("cannot assign to '{}': not a variable", identifier).into());
                }
                None => {
                    return Err(
                        format!("assignment to undeclared variable '{}'", identifier).into(),
                    );
                }
            };
            let e = expr_to_typed(expr, current_scope, generic_cache)?;
            let e = check_assignable(
                &target_ty,
                e,
                current_scope,
                &format!("'{}' of type {:?}", identifier, target_ty),
            )?;
            Ok(TypedStatement::Assign {
                name: identifier,
                expr: e,
            })
        }
        StatementKind::FieldAssign {
            object,
            field,
            expr,
        } => {
            // Lower the object expression — its inferred type tells us the struct
            // shape, which we use to compute the field index.
            // If the object is a plain identifier referring to an immutable
            // binding, surface a friendly error.
            if let PExprKind::Identifier(id) = &object.kind
                && let Some(TypedSymbol::Binding(b)) = current_scope.lookup(id)
                && !b.mutable
            {
                return Err(format!("cannot assign to field of constant '{}'", id).into());
            }
            let obj_typed = expr_to_typed(object, current_scope, generic_cache)?;
            let struct_fields = match &obj_typed.inferred_type {
                Ty::Struct { fields } => fields.clone(),
                Ty::Identifier(_) => {
                    resolve_struct_fields(&obj_typed.inferred_type, current_scope)?
                }
                other => {
                    return Err(format!(
                        "stmt_to_typed: FieldAssign target is not a struct: {:?}",
                        other
                    )
                    .into());
                }
            };
            let (field_index, field_ty) = struct_fields
                .iter()
                .enumerate()
                .find_map(|(i, (name, ty))| (name == &field.value).then_some((i, ty)))
                .ok_or_else(|| {
                    format!("stmt_to_typed: field {} not found in struct", field.value)
                })?;
            let e = expr_to_typed(expr, current_scope, generic_cache)?;
            // Coerce the RHS to the field type via the shared assignment check.
            let e = check_assignable(
                field_ty,
                e,
                current_scope,
                &format!("field '{}' of type {:?}", field.value, field_ty),
            )?;
            Ok(TypedStatement::FieldAssign {
                object: obj_typed,
                field: field.value,
                field_index,
                expr: e,
            })
        }
        StatementKind::ExpressionStatement(e) => Ok(TypedStatement::Expr(expr_to_typed(
            e,
            current_scope,
            generic_cache,
        )?)),
        StatementKind::MultiAssignment { targets, values } => {
            let mut typed_targets = Vec::new();
            for target in targets {
                validate_assignment_target(&target, current_scope)?;
                typed_targets.push(expr_to_typed(target, current_scope, generic_cache)?);
            }
            let mut typed_values = Vec::new();
            for value in values {
                typed_values.push(expr_to_typed(value, current_scope, generic_cache)?);
            }
            validate_multi_assignment_shape(&typed_targets, &typed_values)?;
            Ok(TypedStatement::MultiAssign {
                targets: typed_targets,
                values: typed_values,
            })
        }
        StatementKind::MultiVariableDeclaration {
            identifiers,
            values,
        } => multi_var_decl_to_typed(identifiers, values, current_scope, generic_cache),
        StatementKind::Return(opt) => Ok(TypedStatement::Return(match opt {
            Some(exprs) => {
                let mut typed_exprs = Vec::new();
                for expr in exprs {
                    typed_exprs.push(expr_to_typed(expr, current_scope, generic_cache)?);
                }
                Some(TypedReturn {
                    values: typed_exprs,
                })
            }
            None => None,
        })),
        StatementKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            let cond = expr_to_typed(condition, current_scope, generic_cache)?;
            // Each branch gets its own block scope so bindings declared
            // inside it don't leak into the enclosing block.
            current_scope.enter_scope(ScopeKind::Block);
            let mut then_h = Vec::new();
            for s in then_branch {
                then_h.push(stmt_to_typed_at(
                    s,
                    current_scope,
                    generic_cache,
                    loop_depth,
                )?);
            }
            current_scope.exit_scope();
            let else_h = match else_branch {
                Some(v) => {
                    current_scope.enter_scope(ScopeKind::Block);
                    let mut ev = Vec::new();
                    for s in v {
                        ev.push(stmt_to_typed_at(
                            s,
                            current_scope,
                            generic_cache,
                            loop_depth,
                        )?);
                    }
                    current_scope.exit_scope();
                    Some(ev)
                }
                None => None,
            };
            Ok(TypedStatement::If(TypedIf {
                cond,
                then_branch: then_h,
                else_branch: else_h,
            }))
        }
        StatementKind::For {
            initializer,
            condition,
            post_operation: increment,
            body,
        } => {
            // The loop header and body share one block scope (the init
            // binding is visible to cond/post/body) that doesn't leak out.
            current_scope.enter_scope(ScopeKind::Block);
            let init_h = match initializer {
                Some(b) => Some(Box::new(stmt_to_typed_at(
                    *b,
                    current_scope,
                    generic_cache,
                    loop_depth,
                )?)),
                None => None,
            };
            let cond_h = match condition {
                Some(e) => Some(expr_to_typed(e, current_scope, generic_cache)?),
                None => None,
            };
            let post_h = match increment {
                Some(b) => Some(Box::new(stmt_to_typed_at(
                    *b,
                    current_scope,
                    generic_cache,
                    loop_depth,
                )?)),
                None => None,
            };
            let mut body_h = Vec::new();
            for s in body {
                body_h.push(stmt_to_typed_at(
                    s,
                    current_scope,
                    generic_cache,
                    loop_depth + 1,
                )?);
            }
            current_scope.exit_scope();
            Ok(TypedStatement::For {
                init: init_h,
                cond: cond_h,
                post: post_h,
                body: body_h,
            })
        }
        StatementKind::DerefAssign { pointer, expr } => {
            let ptr_typed = expr_to_typed(pointer, current_scope, generic_cache)?;
            let pointee_ty = match &ptr_typed.inferred_type {
                Ty::Pointer(pointee) => *pointee.clone(),
                other => {
                    return Err(format!(
                        "stmt_to_typed: DerefAssign pointer expression has non-pointer type {:?}",
                        other
                    )
                    .into());
                }
            };
            let val_typed = expr_to_typed(expr, current_scope, generic_cache)?;
            // Coerce the RHS to the pointee type via the shared assignment check.
            let val_typed = check_assignable(
                &pointee_ty,
                val_typed,
                current_scope,
                &format!("pointee of type {:?} through pointer", pointee_ty),
            )?;
            Ok(TypedStatement::DerefAssign {
                pointer: ptr_typed,
                expr: val_typed,
            })
        }
        StatementKind::IndexAssign {
            object,
            index,
            expr,
        } => {
            let obj_typed = expr_to_typed(object, current_scope, generic_cache)?;
            let elem_ty = match &obj_typed.inferred_type {
                Ty::Pointer(inner) => inner.as_ref().clone(),
                Ty::Array { element_type, .. } => element_type.as_ref().clone(),
                other => {
                    return Err(format!(
                        "stmt_to_typed: IndexAssign on non-pointer type {:?}",
                        other
                    )
                    .into());
                }
            };
            let idx_typed = expr_to_typed(index, current_scope, generic_cache)?;
            require_integer_index(&idx_typed.inferred_type, "index assignment")?;
            let val_typed = expr_to_typed(expr, current_scope, generic_cache)?;
            // Coerce the value to the element type via the shared assignment check.
            let val_typed = check_assignable(
                &elem_ty,
                val_typed,
                current_scope,
                &format!("array element of type {:?}", elem_ty),
            )?;
            Ok(TypedStatement::IndexAssign {
                object: obj_typed,
                index: idx_typed,
                expr: val_typed,
            })
        }
        StatementKind::Break => {
            if loop_depth == 0 {
                return Err("`break` is only allowed inside a loop".to_string().into());
            }
            Ok(TypedStatement::Break)
        }
        StatementKind::Continue => {
            if loop_depth == 0 {
                return Err("`continue` is only allowed inside a loop"
                    .to_string()
                    .into());
            }
            Ok(TypedStatement::Continue)
        }
        StatementKind::Defer(inner) => {
            let typed_inner = stmt_to_typed_at(*inner, current_scope, generic_cache, loop_depth)?;
            Ok(TypedStatement::Defer(Box::new(typed_inner)))
        }
        StatementKind::Switch { subject, arms } => {
            let subj_typed = expr_to_typed(subject, current_scope, generic_cache)?;
            let resolved = resolve_type_alias(subj_typed.inferred_type.clone(), current_scope);
            let variants = match &resolved {
                Ty::Enum { variants } => variants.clone(),
                other => {
                    return Err(format!(
                        "stmt_to_typed: switch subject is not an enum: {:?}",
                        other
                    )
                    .into());
                }
            };
            let mut typed_arms = Vec::new();
            for arm in arms {
                let (typed_pattern, arm_binding) = match arm.pattern {
                    Pattern::Wildcard => (TypedPattern::Wildcard, None),
                    Pattern::EnumVariant { variant, binding } => {
                        let v = variants
                            .iter()
                            .find(|v| v.name == variant.value)
                            .ok_or_else(|| {
                                format!(
                                    "stmt_to_typed: enum has no variant '{}' in switch arm",
                                    variant.value
                                )
                            })?;
                        let payload_ty = v.payload.as_ref().map(|fields| Ty::Struct {
                            fields: fields
                                .iter()
                                .map(|(n, t)| (n.clone(), Box::new(t.clone())))
                                .collect(),
                        });
                        if binding.is_some() && payload_ty.is_none() {
                            return Err(format!(
                                "switch arm: variant '{}' has no payload but a binding was supplied",
                                variant
                            )
                            .into());
                        }
                        let bind_clone = binding.clone();
                        (
                            TypedPattern::EnumVariant {
                                variant: v.name.clone(),
                                discriminant: v.discriminant,
                                binding,
                                payload_ty: payload_ty.clone(),
                            },
                            bind_clone.zip(payload_ty),
                        )
                    }
                };
                // Each arm body gets its own block scope; the pattern binding
                // (if any) only exists inside it.
                current_scope.enter_scope(ScopeKind::Block);
                if let Some((bind_id, payload_ty)) = &arm_binding {
                    current_scope.insert(
                        bind_id.clone(),
                        TypedSymbol::Binding(TypedBinding {
                            name: bind_id.clone(),
                            ty: payload_ty.clone(),
                            init: None,
                            mutable: false,
                        }),
                    );
                }
                let mut body_h = Vec::new();
                for s in arm.body {
                    body_h.push(stmt_to_typed_at(
                        s,
                        current_scope,
                        generic_cache,
                        loop_depth,
                    )?);
                }
                current_scope.exit_scope();
                typed_arms.push(TypedSwitchArm {
                    pattern: typed_pattern,
                    body: body_h,
                });
            }
            // Exhaustiveness: every variant must be covered unless a wildcard
            // arm exists. The wildcard itself is checked at typetime, so an
            // arm list with no wildcard and missing variants is a compile error.
            let has_wildcard = typed_arms
                .iter()
                .any(|arm| matches!(arm.pattern, TypedPattern::Wildcard));
            if !has_wildcard {
                let covered: Vec<u32> = typed_arms
                    .iter()
                    .filter_map(|arm| match &arm.pattern {
                        TypedPattern::EnumVariant { discriminant, .. } => Some(*discriminant),
                        TypedPattern::Wildcard => None,
                    })
                    .collect();
                let missing: Vec<String> = variants
                    .iter()
                    .filter(|v| !covered.contains(&v.discriminant))
                    .map(|v| v.name.clone())
                    .collect();
                if !missing.is_empty() {
                    return Err(format!(
                        "switch on enum is not exhaustive: missing variant(s) {} (add `when _` or cover them)",
                        missing.join(", ")
                    )
                    .into());
                }
            }
            Ok(TypedStatement::Switch {
                subject: subj_typed,
                arms: typed_arms,
            })
        }
    }
}

pub(super) fn validate_assignment_target(
    target: &PExpr,
    current_scope: &SymbolTable,
) -> Result<(), AnalysisError> {
    match &target.kind {
        PExprKind::Identifier(id) => {
            if let Some(TypedSymbol::Binding(binding)) = current_scope.lookup(id)
                && !binding.mutable
            {
                return Err(format!("cannot assign to constant '{}'", id).into());
            }
            Ok(())
        }
        PExprKind::FieldAccess { object, .. } => {
            if let PExprKind::Identifier(id) = &object.kind
                && let Some(TypedSymbol::Binding(binding)) = current_scope.lookup(id)
                && !binding.mutable
            {
                return Err(format!("cannot assign to field of constant '{}'", id).into());
            }
            Ok(())
        }
        PExprKind::Dereference(_) | PExprKind::IndexAccess { .. } => Ok(()),
        _ => Err("invalid multi-assignment target".to_string().into()),
    }
}

/// Shared arity rule for `a, b = ...` (multi-assignment) and `q, r := ...`
/// (multi-variable declaration): either one value per target, or a single
/// tuple-typed value with one element per target. Returns the per-target
/// types on success, `None` on arity mismatch (callers attach their own
/// `multi-assignment` / `multi-variable declaration` wording).
fn resolve_multi_value_types(target_count: usize, values: &[TypedExpr]) -> Option<Vec<Ty>> {
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

pub(super) fn validate_multi_assignment_shape(
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

pub(super) fn infer_multi_binding_types(
    identifiers: &[Identifier],
    values: &[TypedExpr],
) -> Result<Vec<Ty>, AnalysisError> {
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

pub(super) fn multi_var_decl_to_typed(
    identifiers: Vec<Identifier>,
    values: Vec<PExpr>,
    current_scope: &mut SymbolTable,
    generic_cache: &mut HashMap<String, TypedFunction>,
) -> Result<TypedStatement, AnalysisError> {
    let mut typed_values = Vec::new();
    for value in values {
        typed_values.push(expr_to_typed(value, current_scope, generic_cache)?);
    }

    let binding_types = infer_multi_binding_types(&identifiers, &typed_values)?;
    let mut bindings = Vec::new();
    for (identifier, ty) in identifiers.into_iter().zip(binding_types) {
        let binding = TypedBinding {
            name: identifier.clone(),
            ty,
            init: None,
            mutable: true,
        };
        current_scope.insert(identifier, TypedSymbol::Binding(binding.clone()));
        bindings.push(binding);
    }

    Ok(TypedStatement::MultiBinding {
        bindings,
        values: typed_values,
    })
}

pub(super) fn var_decl_to_typed(
    var_decl: VariableDeclaration,
    current_scope: &mut SymbolTable,
    generic_cache: &mut HashMap<String, TypedFunction>,
) -> Result<TypedBinding, AnalysisError> {
    let declared_ty = match var_decl.constant_type {
        Some(t) if matches!(&t.kind, TypeExpressionKind::TypeKeyword) => {
            return Err("mutable type bindings (`var type`) are not yet supported; use `const type` for compile-time type aliases".to_string().into());
        }
        Some(t) => Some(map_type(t)?),
        None => None,
    };
    let init = match var_decl.expression {
        Some(expr) => Some(expr_to_typed(expr, current_scope, generic_cache)?),
        // A typed declaration without an initializer is legal: the binding
        // stays uninitialized until first assignment.
        None if declared_ty.is_some() => None,
        None => {
            return Err("inferred variable declaration requires an initializer"
                .to_string()
                .into());
        }
    };
    let ty = match (declared_ty, &init) {
        (Some(ty), _) => ty,
        (None, Some(init)) => init.inferred_type.clone(),
        // Defensive: the `None` arm above already rejected a missing
        // initializer for inferred declarations — never panic here.
        (None, None) => {
            return Err(AnalysisError::from(
                "inferred variable declaration requires an initializer".to_string(),
            )
            .with_hint(
                "add an initializer, or declare an explicit type and defer initialization, e.g. `var @int4 x;`",
            ));
        }
    };
    // Check the initializer against the declared/inferred type. Array
    // initializers need their size checked first (`coerce_to` never compares
    // sizes); everything else goes through the single `coerce_to` rule
    // (`Never`/`null` re-annotates, aliases match structurally, numerics
    // relabel literals or insert `Cast`).
    let init = match init {
        None => None,
        Some(init) => {
            if let (
                Ty::Array {
                    element_type: decl_elem,
                    size: decl_size,
                },
                Ty::Array {
                    size: init_size, ..
                },
            ) = (&ty, &init.inferred_type)
            {
                if *decl_size != *init_size {
                    return Err(format!(
                        "array size mismatch for {}: declared size {} but initializer has {} elements",
                        var_decl.identifier, decl_size, init_size
                    )
                    .into());
                }
                let mut init = init;
                // Coerce each literal element to the declared element type
                // with the shared rule instead of blindly relabeling, which
                // generated wrong code for widening element types.
                if let TypedExprKind::ArrayLiteral { elements } = &mut init.expression {
                    let mut coerced = Vec::with_capacity(elements.len());
                    for elem in std::mem::take(elements) {
                        let found = elem.inferred_type.clone();
                        coerced.push(coerce_to(decl_elem, elem, current_scope).map_err(|_| {
                            format!(
                                "array element of type {:?} does not match declared element type {:?} for {}",
                                found, decl_elem, var_decl.identifier
                            )
                        })?);
                    }
                    *elements = coerced;
                }
                init.inferred_type = ty.clone();
                Some(init)
            } else if init.inferred_type == ty {
                Some(init)
            } else {
                let found = init.inferred_type.clone();
                let init = coerce_to(&ty, init, current_scope).map_err(|_| {
                    format!(
                        "initialization type {:?} does not match declared type {:?} for {}",
                        found, ty, var_decl.identifier
                    )
                })?;
                Some(init)
            }
        }
    };
    let typed_var = TypedBinding {
        name: var_decl.identifier.clone(),
        ty,
        init,
        mutable: true,
    };
    current_scope.insert(var_decl.identifier, TypedSymbol::Binding(typed_var.clone()));
    Ok(typed_var)
}
