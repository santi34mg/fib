use std::collections::HashMap;

use crate::frontend::ast::pattern::Pattern;
use crate::frontend::ast::{
    expression::Expression as PExpr, statement::Statement, statement::StatementKind,
    type_expression::TypeExpression, variable_declaration::VariableDeclaration,
};
use crate::frontend::identifier::Identifier;
use crate::frontend::tokens::builtin::BuiltinType;
use crate::frontend::typed_ast::{
    ScopeKind, SymbolTable, Ty, TypedBinding, TypedExpr, TypedExprKind, TypedFunction, TypedIf,
    TypedPattern, TypedReturn, TypedStatement, TypedSwitchArm, TypedSymbol,
};

use super::AnalysisError;
use super::expressions::{
    coerce_expr_to_type, coerce_or_alias, expr_to_typed, require_integer_index,
};
use super::types::{map_type, resolve_struct_fields, resolve_type_alias};

pub(super) fn stmt_to_typed(
    stmt: Statement,
    current_scope: &mut SymbolTable,
    generic_cache: &mut HashMap<String, TypedFunction>,
) -> Result<TypedStatement, AnalysisError> {
    let line = stmt.line;
    stmt_to_typed_inner(stmt.kind, current_scope, generic_cache).map_err(|e| e.with_line(line))
}

pub(super) fn stmt_to_typed_inner(
    stmt: StatementKind,
    current_scope: &mut SymbolTable,
    generic_cache: &mut HashMap<String, TypedFunction>,
) -> Result<TypedStatement, AnalysisError> {
    match stmt {
        StatementKind::VariableDeclaration(variable_declaration) => Ok(TypedStatement::Binding(
            var_decl_to_typed(variable_declaration, current_scope, generic_cache)?,
        )),
        StatementKind::Assignment { identifier, expr } => {
            let target_ty = match current_scope.lookup(&identifier) {
                Some(TypedSymbol::Binding(binding)) => {
                    if !binding.mutable {
                        return Err(format!("cannot assign to constant '{}'", identifier).into());
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
            let found = e.inferred_type.clone();
            let e = coerce_or_alias(e, &target_ty, current_scope).map_err(|_| {
                format!(
                    "cannot assign value of type {:?} to '{}' of type {:?}",
                    found, identifier, target_ty
                )
            })?;
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
            if let PExpr::Identifier(id) = &object
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
            // Coerce the RHS to the field type, mirroring plain `Assign`.
            let found = e.inferred_type.clone();
            let e = coerce_or_alias(e, field_ty, current_scope).map_err(|_| {
                format!(
                    "cannot assign value of type {:?} to field '{}' of type {:?}",
                    found, field.value, field_ty
                )
            })?;
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
                then_h.push(stmt_to_typed(s, current_scope, generic_cache)?);
            }
            current_scope.exit_scope();
            let else_h = match else_branch {
                Some(v) => {
                    current_scope.enter_scope(ScopeKind::Block);
                    let mut ev = Vec::new();
                    for s in v {
                        ev.push(stmt_to_typed(s, current_scope, generic_cache)?);
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
                Some(b) => Some(Box::new(stmt_to_typed(*b, current_scope, generic_cache)?)),
                None => None,
            };
            let cond_h = match condition {
                Some(e) => Some(expr_to_typed(e, current_scope, generic_cache)?),
                None => None,
            };
            let post_h = match increment {
                Some(b) => Some(Box::new(stmt_to_typed(*b, current_scope, generic_cache)?)),
                None => None,
            };
            let mut body_h = Vec::new();
            for s in body {
                body_h.push(stmt_to_typed(s, current_scope, generic_cache)?);
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
            // Coerce the RHS to the pointee type, mirroring plain `Assign`
            // (previously the pointee type was discarded unchecked).
            let found = val_typed.inferred_type.clone();
            let val_typed =
                coerce_or_alias(val_typed, &pointee_ty, current_scope).map_err(|_| {
                    format!(
                        "cannot assign value of type {:?} through pointer (expects {:?})",
                        found, pointee_ty
                    )
                })?;
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
            // Coerce the value to the element type, mirroring plain `Assign`.
            let found = val_typed.inferred_type.clone();
            let val_typed = coerce_or_alias(val_typed, &elem_ty, current_scope).map_err(|_| {
                format!(
                    "cannot assign value of type {:?} to array element of type {:?}",
                    found, elem_ty
                )
            })?;
            Ok(TypedStatement::IndexAssign {
                object: obj_typed,
                index: idx_typed,
                expr: val_typed,
            })
        }
        StatementKind::Break => Ok(TypedStatement::Break),
        StatementKind::Continue => Ok(TypedStatement::Continue),
        StatementKind::Defer(inner) => {
            let typed_inner = stmt_to_typed(*inner, current_scope, generic_cache)?;
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
                    body_h.push(stmt_to_typed(s, current_scope, generic_cache)?);
                }
                current_scope.exit_scope();
                typed_arms.push(TypedSwitchArm {
                    pattern: typed_pattern,
                    body: body_h,
                });
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
    match target {
        PExpr::Identifier(id) => {
            if let Some(TypedSymbol::Binding(binding)) = current_scope.lookup(id)
                && !binding.mutable
            {
                return Err(format!("cannot assign to constant '{}'", id).into());
            }
            Ok(())
        }
        PExpr::FieldAccess { object, .. } => {
            if let PExpr::Identifier(id) = object.as_ref()
                && let Some(TypedSymbol::Binding(binding)) = current_scope.lookup(id)
                && !binding.mutable
            {
                return Err(format!("cannot assign to field of constant '{}'", id).into());
            }
            Ok(())
        }
        PExpr::Dereference(_) | PExpr::IndexAccess { .. } => Ok(()),
        _ => Err("invalid multi-assignment target".to_string().into()),
    }
}

pub(super) fn validate_multi_assignment_shape(
    targets: &[TypedExpr],
    values: &[TypedExpr],
) -> Result<(), AnalysisError> {
    if values.len() == targets.len() {
        return Ok(());
    }
    if values.len() == 1
        && let Ty::Tuple { elements } = &values[0].inferred_type
        && elements.len() == targets.len()
    {
        return Ok(());
    }
    Err(format!(
        "multi-assignment arity mismatch: {} target(s), {} value expression(s)",
        targets.len(),
        values.len()
    )
    .into())
}

pub(super) fn infer_multi_binding_types(
    identifiers: &[Identifier],
    values: &[TypedExpr],
) -> Result<Vec<Ty>, AnalysisError> {
    if values.len() == identifiers.len() {
        return Ok(values
            .iter()
            .map(|value| value.inferred_type.clone())
            .collect());
    }
    if values.len() == 1
        && let Ty::Tuple { elements } = &values[0].inferred_type
        && elements.len() == identifiers.len()
    {
        return Ok(elements.clone());
    }
    Err(format!(
        "multi-variable declaration arity mismatch: {} identifier(s), {} value expression(s)",
        identifiers.len(),
        values.len()
    )
    .into())
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
        Some(TypeExpression::TypeKeyword) => {
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
            return Err("inferred variable declaration requires an initializer"
                .to_string()
                .into());
        }
    };
    // check that type matches; allow struct-by-name to match struct literal type
    // `never` unifies with any type
    let init = match init {
        None => None,
        Some(mut init) => {
            if init.inferred_type == Ty::Builtin(BuiltinType::Never) {
                init.inferred_type = ty.clone();
            }
            if init.inferred_type != ty {
                // When the declared type is an identifier (e.g. `Point`) and the init
                // expression is a StructConstruct, the inferred type is the resolved
                // Ty::Struct directly.  In that case, accept the match by
                // annotating the init expression with the identifier type so the rest
                // of the pipeline sees a consistent declared type.
                let resolved_ty_match = match &ty {
                    Ty::Identifier(id) => match current_scope.lookup(id) {
                        Some(TypedSymbol::Type(inner)) => *inner == init.inferred_type,
                        _ => false,
                    },
                    _ => false,
                };
                if resolved_ty_match {
                    // Replace the init's inferred type with the declared identifier type
                    // so that subsequent lookups (e.g. binding type in codegen) return
                    // the identifier-keyed type.
                    init.inferred_type = ty.clone();
                } else if let (
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
                    // Coerce each element to the declared element type with
                    // the shared numeric/alias rule (literals relabel,
                    // numerics insert `Cast`) instead of blindly relabeling,
                    // which generated wrong code for widening element types.
                    if let TypedExprKind::ArrayLiteral { elements } = &mut init.expression {
                        let mut coerced = Vec::with_capacity(elements.len());
                        for elem in std::mem::take(elements) {
                            let found = elem.inferred_type.clone();
                            coerced.push(
                                coerce_or_alias(elem, decl_elem, current_scope).map_err(|_| {
                                    format!(
                                        "array element of type {:?} does not match declared element type {:?} for {}",
                                        found, decl_elem, var_decl.identifier
                                    )
                                })?,
                            );
                        }
                        *elements = coerced;
                    }
                    init.inferred_type = ty.clone();
                } else if let Ty::Builtin(_) = &ty {
                    // Numeric mismatches get a real conversion (literals are just
                    // re-typed; other expressions get a cast); anything else errors.
                    let found = init.inferred_type.clone();
                    init = coerce_expr_to_type(init, &ty).map_err(|_| {
                        format!(
                            "initialization type {:?} does not match declared type {:?} for {}",
                            found, ty, var_decl.identifier
                        )
                    })?;
                } else {
                    return Err(format!(
                        r#"initalization type does not match explicit type for {}
explicit type: {:?}
inferred type of expression: {:?}"#,
                        var_decl.identifier, ty, init.inferred_type
                    )
                    .into());
                }
            }
            Some(init)
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
