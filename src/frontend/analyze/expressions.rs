use std::collections::HashMap;

use crate::frontend::ast::expression::Expression as PExpr;
use crate::frontend::identifier::Identifier;
use crate::frontend::tokens::Operator;
use crate::frontend::tokens::builtin::{BuiltinFunction, BuiltinType};
use crate::frontend::tokens::literal::Literal;
use crate::frontend::typed_ast::{
    BinOp, LogicalOp, SymbolTable, Ty, TypedExpr, TypedExprKind, TypedFunction, TypedSymbol,
};

use super::AnalysisError;
use super::generics::instantiate_generic;
use super::types::{map_type, resolve_struct_fields, resolve_type_alias};

pub(super) fn is_integer_builtin(builtin: &BuiltinType) -> bool {
    matches!(
        builtin,
        BuiltinType::Int1
            | BuiltinType::Int2
            | BuiltinType::Int4
            | BuiltinType::Int8
            | BuiltinType::Int16
            | BuiltinType::UInt1
            | BuiltinType::UInt2
            | BuiltinType::UInt4
            | BuiltinType::UInt8
            | BuiltinType::UInt16
            | BuiltinType::Char
    )
}

pub(super) fn is_float_builtin(builtin: &BuiltinType) -> bool {
    matches!(
        builtin,
        BuiltinType::Float2 | BuiltinType::Float4 | BuiltinType::Float8 | BuiltinType::Float16
    )
}

pub(super) fn is_numeric_type(ty: &Ty) -> bool {
    matches!(
        ty,
        Ty::Builtin(builtin) if is_integer_builtin(builtin) || is_float_builtin(builtin)
    )
}

pub(super) fn is_boolean_type(ty: &Ty) -> bool {
    matches!(ty, Ty::Builtin(BuiltinType::Boolean))
}

/// Reject non-integer index expressions (floats, strings, pointers...).
/// Integer widths need no coercion: lowering passes any int to `gep`.
pub(super) fn require_integer_index(ty: &Ty, what: &str) -> Result<(), AnalysisError> {
    match ty {
        Ty::Builtin(b) if is_integer_builtin(b) => Ok(()),
        _ => Err(format!("{} requires an integer index, found {:?}", what, ty).into()),
    }
}

pub(super) fn coerce_expr_to_type(
    mut expr: TypedExpr,
    target: &Ty,
) -> Result<TypedExpr, AnalysisError> {
    if &expr.inferred_type == target {
        return Ok(expr);
    }

    if expr.inferred_type == Ty::Builtin(BuiltinType::Never) {
        expr.inferred_type = target.clone();
        return Ok(expr);
    }

    match (&expr.expression, &expr.inferred_type, target) {
        (TypedExprKind::LiteralInt { .. }, Ty::Builtin(src), Ty::Builtin(dst))
            if is_integer_builtin(src) && is_integer_builtin(dst) =>
        {
            expr.inferred_type = target.clone();
            Ok(expr)
        }
        (TypedExprKind::LiteralFloat { .. }, Ty::Builtin(src), Ty::Builtin(dst))
            if is_float_builtin(src) && is_float_builtin(dst) =>
        {
            expr.inferred_type = target.clone();
            Ok(expr)
        }
        (_, src, dst) if is_numeric_type(src) && is_numeric_type(dst) => Ok(TypedExpr {
            inferred_type: target.clone(),
            expression: TypedExprKind::Cast {
                expr: Box::new(expr),
                target_type: target.clone(),
            },
        }),
        _ => Err(format!(
            "cannot coerce expression of type {:?} to {:?}",
            expr.inferred_type, target
        )
        .into()),
    }
}

/// Like `coerce_expr_to_type`, but also accepts the case where the target is
/// a named type alias of the expression's structural type (or vice versa),
/// in which case the expression is just re-annotated with the target type.
pub(super) fn coerce_or_alias(
    expr: TypedExpr,
    target: &Ty,
    scope: &SymbolTable,
) -> Result<TypedExpr, AnalysisError> {
    if &expr.inferred_type == target {
        return Ok(expr);
    }
    if resolve_type_alias(target.clone(), scope)
        == resolve_type_alias(expr.inferred_type.clone(), scope)
    {
        let mut e = expr;
        e.inferred_type = target.clone();
        return Ok(e);
    }
    coerce_expr_to_type(expr, target)
}

pub(super) fn check_call_args(
    func_name: &str,
    params: &[(Identifier, Ty)],
    is_variadic: bool,
    args: Vec<PExpr>,
    current_scope: &SymbolTable,
    generic_cache: &mut HashMap<String, TypedFunction>,
) -> Result<Vec<TypedExpr>, AnalysisError> {
    if is_variadic {
        if args.len() < params.len() {
            return Err(format!(
                "{} expects at least {} argument(s), but {} were given",
                func_name,
                params.len(),
                args.len()
            )
            .into());
        }
    } else if args.len() != params.len() {
        return Err(format!(
            "{} expects {} argument(s), but {} were given",
            func_name,
            params.len(),
            args.len()
        )
        .into());
    }

    let mut hargs = Vec::with_capacity(args.len());
    for (i, arg) in args.into_iter().enumerate() {
        let harg = expr_to_typed(arg, current_scope, generic_cache)?;
        let harg = match params.get(i) {
            Some((param_name, param_type)) => {
                let found_type = harg.inferred_type.clone();
                coerce_expr_to_type(harg, param_type).map_err(|_| {
                    format!(
                        "{}: argument '{}' expects type {:?}, but found {:?}",
                        func_name, param_name.value, param_type, found_type
                    )
                })?
            }
            // Extra arguments to a variadic function (e.g. printf) aren't type-checked.
            None => harg,
        };
        hargs.push(harg);
    }
    Ok(hargs)
}

pub(super) fn expr_to_typed(
    expr: PExpr,
    current_scope: &SymbolTable,
    generic_cache: &mut HashMap<String, TypedFunction>,
) -> Result<TypedExpr, AnalysisError> {
    match expr {
        PExpr::Literal(Literal::Integer(value)) => {
            // Default to Int32 (i32); explicit type annotations coerce as needed.
            Ok(TypedExpr {
                inferred_type: Ty::Builtin(BuiltinType::Int4),
                expression: TypedExprKind::LiteralInt { value },
            })
        }
        PExpr::Literal(Literal::Boolean(b)) => Ok(TypedExpr {
            inferred_type: Ty::Builtin(BuiltinType::Boolean),
            expression: TypedExprKind::LiteralBool(b),
        }),
        PExpr::Literal(Literal::Float(f)) => Ok(TypedExpr {
            inferred_type: Ty::Builtin(BuiltinType::Float8),
            expression: TypedExprKind::LiteralFloat { value: f },
        }),
        PExpr::Literal(Literal::Character(c)) => Ok(TypedExpr {
            inferred_type: Ty::Builtin(BuiltinType::Char),
            expression: TypedExprKind::LiteralInt { value: c as u64 },
        }),
        PExpr::Literal(Literal::String(raw)) => {
            let value = process_escape_sequences(&raw)?;
            Ok(TypedExpr {
                inferred_type: Ty::Builtin(BuiltinType::String),
                expression: TypedExprKind::LiteralString { value },
            })
        }
        // `null` unifies with any type (like `never`); it is concretized by
        // the context it is used in (declaration type, comparison operand, ...).
        PExpr::Literal(Literal::Null) => Ok(TypedExpr {
            inferred_type: Ty::Builtin(BuiltinType::Never),
            expression: TypedExprKind::Null,
        }),
        PExpr::BuiltinCall { builtin, args } => {
            // Every string builtin takes `@string` arguments; only the arity and
            // return type differ.
            let (arity, ret) = match builtin {
                BuiltinFunction::StrLen => (1usize, Ty::Builtin(BuiltinType::UInt8)),
                BuiltinFunction::Concat => (2, Ty::Builtin(BuiltinType::String)),
                BuiltinFunction::StrEq => (2, Ty::Builtin(BuiltinType::Boolean)),
            };
            if args.len() != arity {
                return Err(format!(
                    "{} expects {} argument(s), but {} were given",
                    builtin,
                    arity,
                    args.len()
                )
                .into());
            }
            let string_ty = Ty::Builtin(BuiltinType::String);
            let mut hargs = Vec::with_capacity(args.len());
            for (i, arg) in args.into_iter().enumerate() {
                let harg = expr_to_typed(arg, current_scope, generic_cache)?;
                let found = harg.inferred_type.clone();
                let harg = coerce_expr_to_type(harg, &string_ty).map_err(|_| -> AnalysisError {
                    format!(
                        "{}: argument {} expects type @string, but found {:?}",
                        builtin,
                        i + 1,
                        found
                    )
                    .into()
                })?;
                hargs.push(harg);
            }
            Ok(TypedExpr {
                inferred_type: ret,
                expression: TypedExprKind::BuiltinCall {
                    builtin,
                    args: hargs,
                },
            })
        }
        PExpr::Identifier(name) => {
            let sym = current_scope
                .lookup(&name)
                .ok_or_else(|| format!("expr_to_typed: identifier {} not found in scope", name))?;
            match sym {
                TypedSymbol::Binding(var) => Ok(TypedExpr {
                    inferred_type: var.ty.clone(),
                    expression: TypedExprKind::Identifier(name),
                }),
                TypedSymbol::Type(ty) => {
                    // A type name used in expression position is a comptime type value.
                    Ok(TypedExpr {
                        inferred_type: Ty::Type,
                        expression: TypedExprKind::ComptimeType(ty.clone()),
                    })
                }
                TypedSymbol::GenericFunction(_) | TypedSymbol::Function(_) => {
                    Err(format!("expr_to_typed: identifier {} is not a variable", name).into())
                }
            }
        }
        PExpr::EnumVariantConstruct {
            type_name,
            variant,
            fields,
        } => {
            let sym = current_scope.lookup(&type_name).ok_or_else(|| {
                format!(
                    "expr_to_typed: type '{}' not found for enum variant construction",
                    type_name
                )
            })?;
            let TypedSymbol::Type(enum_ty) = sym else {
                return Err(format!("expr_to_typed: '{}' is not a type", type_name).into());
            };
            let resolved = resolve_type_alias(enum_ty.clone(), current_scope);
            let Ty::Enum { variants } = &resolved else {
                return Err(format!("expr_to_typed: '{}' is not an enum type", type_name).into());
            };
            let v = variants
                .iter()
                .find(|v| v.name == variant.value)
                .ok_or_else(|| {
                    format!(
                        "expr_to_typed: enum '{}' has no variant '{}'",
                        type_name, variant
                    )
                })?
                .clone();
            let Some(payload_spec) = v.payload.clone() else {
                return Err(format!(
                    "expr_to_typed: variant '{}.{}' carries no payload",
                    type_name, variant
                )
                .into());
            };
            let mut typed_fields: Vec<(String, TypedExpr)> = Vec::new();
            for (fname, fval) in fields {
                let Some((_, spec_ty)) = payload_spec.iter().find(|(n, _)| n == &fname.value)
                else {
                    return Err(format!(
                        "expr_to_typed: variant '{}.{}' has no payload field '{}'",
                        type_name, variant, fname.value
                    )
                    .into());
                };
                let fval_typed = expr_to_typed(fval, current_scope, generic_cache)?;
                let fval_typed =
                    coerce_or_alias(fval_typed, spec_ty, current_scope).map_err(|_| {
                        format!(
                            "expr_to_typed: field '{}' of variant '{}.{}' expects type {:?}",
                            fname.value, type_name, variant, spec_ty
                        )
                    })?;
                typed_fields.push((fname.value.clone(), fval_typed));
            }
            // Verify each declared field is supplied (order-insensitive).
            for (n, _) in &payload_spec {
                if !typed_fields.iter().any(|(fname, _)| fname == n) {
                    return Err(format!(
                        "expr_to_typed: missing field '{}' in variant '{}.{}' payload",
                        n, type_name, variant
                    )
                    .into());
                }
            }
            Ok(TypedExpr {
                inferred_type: Ty::Identifier(type_name.clone()),
                expression: TypedExprKind::EnumVariantConstruct {
                    type_name: type_name.value.clone(),
                    variant: v.name.clone(),
                    discriminant: v.discriminant,
                    fields: typed_fields,
                    enum_type: Box::new(resolved),
                },
            })
        }
        PExpr::TypeValue(te) => {
            let typed_type = map_type(te)?;
            Ok(TypedExpr {
                inferred_type: Ty::Type,
                expression: TypedExprKind::ComptimeType(typed_type),
            })
        }
        PExpr::Binary {
            left,
            operator,
            right,
        } => {
            let l = expr_to_typed(*left, current_scope, generic_cache)?;
            let r = expr_to_typed(*right, current_scope, generic_cache)?;
            let (inferred_type, l, r) = match operator {
                // Arithmetic/bitwise: result type = LHS type.
                Operator::Plus
                | Operator::Minus
                | Operator::Star
                | Operator::Slash
                | Operator::Percent
                | Operator::RightShift
                | Operator::LeftShift
                | Operator::Ampersand
                | Operator::Pipe
                | Operator::Caret => {
                    // For pointer arithmetic, keep pointer type; don't coerce RHS.
                    if matches!(l.inferred_type, Ty::Pointer(_)) {
                        (l.inferred_type.clone(), l, r)
                    } else if is_numeric_type(&l.inferred_type) {
                        let r = coerce_expr_to_type(r, &l.inferred_type)?;
                        (l.inferred_type.clone(), l, r)
                    } else {
                        return Err(format!(
                            "operator {:?} requires numeric operands, got {:?}",
                            operator, l.inferred_type
                        )
                        .into());
                    }
                }
                Operator::GreaterThan
                | Operator::GreaterEqual
                | Operator::LesserThan
                | Operator::LesserEqual => {
                    if !is_numeric_type(&l.inferred_type) {
                        return Err(format!(
                            "operator {:?} requires numeric operands, got {:?}",
                            operator, l.inferred_type
                        )
                        .into());
                    }
                    let r = coerce_expr_to_type(r, &l.inferred_type)?;
                    (Ty::Builtin(BuiltinType::Boolean), l, r)
                }
                Operator::DoubleEquals | Operator::Different => {
                    // Coerce whichever side is more flexible: `null == p`
                    // needs the LHS adapted to the RHS pointer type.
                    if l.inferred_type == Ty::Builtin(BuiltinType::Never)
                        && r.inferred_type != Ty::Builtin(BuiltinType::Never)
                    {
                        let target = r.inferred_type.clone();
                        let l = coerce_expr_to_type(l, &target)?;
                        (Ty::Builtin(BuiltinType::Boolean), l, r)
                    } else {
                        let r = coerce_expr_to_type(r, &l.inferred_type)?;
                        (Ty::Builtin(BuiltinType::Boolean), l, r)
                    }
                }
                Operator::LogicalAnd | Operator::LogicalOr => {
                    if !is_boolean_type(&l.inferred_type) || !is_boolean_type(&r.inferred_type) {
                        return Err(format!(
                            "operator {:?} requires bool operands, got {:?} and {:?}",
                            operator, l.inferred_type, r.inferred_type
                        )
                        .into());
                    }
                    let op = LogicalOp::from_syntax(operator)
                        .ok_or_else(|| format!("unsupported binary operator {:?}", operator))?;
                    return Ok(TypedExpr {
                        inferred_type: Ty::Builtin(BuiltinType::Boolean),
                        expression: TypedExprKind::ShortCircuit {
                            left: Box::new(l),
                            operator: op,
                            right: Box::new(r),
                        },
                    });
                }
                op => return Err(format!("unsupported binary operator {:?}", op).into()),
            };
            // Single mapping point from syntax to semantics: every operator
            // that reaches here (arithmetic, comparison, equality) has a
            // `BinOp`. Anything else is an internal error — the arm above
            // already rejected non-binary operators.
            let operator = BinOp::from_syntax(operator)
                .ok_or_else(|| format!("unsupported binary operator {:?}", operator))?;
            Ok(TypedExpr {
                inferred_type,
                expression: TypedExprKind::Binary {
                    left: Box::new(l),
                    operator,
                    right: Box::new(r),
                },
            })
        }
        PExpr::Grouping(inner) => expr_to_typed(*inner, current_scope, generic_cache),
        PExpr::Call { callee, args } => {
            match *callee {
                PExpr::Identifier(name) => {
                    match current_scope.lookup(&name).cloned() {
                        Some(TypedSymbol::GenericFunction(template)) => {
                            // Generic call: extract comptime type args and instantiate.
                            let (mangled_name, return_type) = instantiate_generic(
                                &template,
                                &args,
                                current_scope,
                                generic_cache,
                            )?;
                            // Build Typed args for runtime (non-comptime) parameters only.
                            let mut hargs = Vec::new();
                            for (i, arg) in args.into_iter().enumerate() {
                                if !template.comptime_params.contains(&i) {
                                    hargs.push(expr_to_typed(arg, current_scope, generic_cache)?);
                                }
                            }
                            Ok(TypedExpr {
                                inferred_type: return_type,
                                expression: TypedExprKind::Call {
                                    callee: Identifier { value: mangled_name },
                                    args: hargs,
                                },
                            })
                        }
                        Some(TypedSymbol::Function(func)) => {
                            let inferred_type = func.return_type.clone();
                            let hargs = check_call_args(
                                &name.value,
                                &func.params,
                                func.is_variadic,
                                args,
                                current_scope,
                                generic_cache,
                            )?;
                            Ok(TypedExpr {
                                inferred_type,
                                expression: TypedExprKind::Call {
                                    callee: name.clone(),
                                    args: hargs,
                                },
                            })
                        }
                        Some(_) => Err(format!(
                            "expr_to_typed: symbol {} is not a function",
                            name
                        ).into()),
                        None => Err(format!(
                            "expr_to_typed: unknown function '{}' — did you forget an extern declaration?",
                            name
                        ).into()),
                    }
                }
                PExpr::QualifiedAccess { module, member } => {
                    let module_alias = module.value.clone();
                    let typed_module = current_scope
                        .lookup_module(&module_alias)
                        .ok_or_else(|| format!("unknown module '{}'", module_alias))?;
                    let func = match typed_module.exports.get(&member) {
                        Some(TypedSymbol::Function(f)) => f.clone(),
                        Some(_) => {
                            return Err(format!(
                                "'{}' in module '{}' is not a function",
                                member, module_alias
                            )
                            .into());
                        }
                        None => {
                            return Err(format!(
                                "'{}' not found in module '{}'",
                                member, module_alias
                            )
                            .into());
                        }
                    };
                    // Extern functions keep their C name; user functions get a mangled name.
                    let callee_name = if func.is_extern {
                        member.clone()
                    } else {
                        Identifier {
                            value: format!("{}__{}", module_alias, member.value),
                        }
                    };
                    let hargs = check_call_args(
                        &format!("{}::{}", module_alias, member.value),
                        &func.params,
                        func.is_variadic,
                        args,
                        current_scope,
                        generic_cache,
                    )?;
                    Ok(TypedExpr {
                        inferred_type: func.return_type.clone(),
                        expression: TypedExprKind::Call {
                            callee: callee_name,
                            args: hargs,
                        },
                    })
                }
                // Only accept identifier/qualified callees
                _ => Err(
                    "expr_to_typed: call target must be an identifier or qualified access"
                        .to_string()
                        .into(),
                ),
            }
        }
        PExpr::FieldAccess { object, field } => {
            // Special case: `TypeName.VariantName` on an enum produces an EnumLiteral.
            if let PExpr::Identifier(type_name) = &*object
                && let Some(TypedSymbol::Type(ty)) = current_scope.lookup(type_name)
            {
                let resolved = resolve_type_alias(ty.clone(), current_scope);
                if let Ty::Enum { variants } = &resolved {
                    let v = variants
                        .iter()
                        .find(|v| v.name == field.value)
                        .ok_or_else(|| {
                            format!(
                                "expr_to_typed: enum {} has no variant {}",
                                type_name, field.value
                            )
                        })?;
                    return Ok(TypedExpr {
                        inferred_type: Ty::Identifier(type_name.clone()),
                        expression: TypedExprKind::EnumLiteral {
                            type_name: type_name.value.clone(),
                            variant: v.name.clone(),
                            discriminant: v.discriminant,
                        },
                    });
                }
            }
            let obj_typed = expr_to_typed(*object, current_scope, generic_cache)?;
            let struct_fields = resolve_struct_fields(&obj_typed.inferred_type, current_scope)?;
            let field_index = struct_fields
                .iter()
                .position(|(name, _)| name == &field.value)
                .ok_or_else(|| {
                    format!(
                        "expr_to_typed: field {} not found in struct type {:?}",
                        field.value, obj_typed.inferred_type
                    )
                })?;
            let field_ty = *struct_fields[field_index].1.clone();
            Ok(TypedExpr {
                inferred_type: field_ty,
                expression: TypedExprKind::FieldAccess {
                    object: Box::new(obj_typed),
                    field: field.value,
                    field_index,
                },
            })
        }
        PExpr::StructConstruct { type_name, fields } => {
            // Look up the struct type in scope
            let struct_ty = match current_scope
                .lookup(&type_name)
                .ok_or_else(|| format!("expr_to_typed: type {} not found in scope", type_name))?
            {
                TypedSymbol::Type(ty) => ty.clone(),
                _ => return Err(format!("expr_to_typed: {} is not a type", type_name).into()),
            };
            let struct_fields = resolve_struct_fields(&struct_ty, current_scope)?;
            // Lower the provided fields, verifying each exists in the struct.
            let mut provided: Vec<(String, TypedExpr)> = Vec::new();
            for (fname, fexpr) in fields {
                if !struct_fields.iter().any(|(name, _)| name == &fname.value) {
                    return Err(format!(
                        "expr_to_typed: field {} not found in struct {}",
                        fname.value, type_name
                    )
                    .into());
                }
                if provided.iter().any(|(n, _)| n == &fname.value) {
                    return Err(format!(
                        "expr_to_typed: duplicate field {} in construction of {}",
                        fname.value, type_name
                    )
                    .into());
                }
                let fval = expr_to_typed(fexpr, current_scope, generic_cache)?;
                provided.push((fname.value, fval));
            }
            // Re-emit the fields in *declared* order (lowering stores them
            // positionally) and require every field to be present.
            let mut typed_fields = Vec::new();
            for (fname, fty) in &struct_fields {
                let pos = provided
                    .iter()
                    .position(|(n, _)| n == fname)
                    .ok_or_else(|| {
                        format!(
                            "expr_to_typed: missing field {} in construction of struct {}",
                            fname, type_name
                        )
                    })?;
                let (n, fval) = provided.remove(pos);
                let fval = coerce_or_alias(fval, fty.as_ref(), current_scope).map_err(|_| {
                    format!(
                        "expr_to_typed: field {} of struct {} expects type {:?}",
                        fname, type_name, fty
                    )
                })?;
                typed_fields.push((n, fval));
            }
            Ok(TypedExpr {
                inferred_type: struct_ty,
                expression: TypedExprKind::StructConstruct {
                    type_name: type_name.value,
                    fields: typed_fields,
                },
            })
        }
        PExpr::AddressOf(inner) => {
            let inner_typed = expr_to_typed(*inner, current_scope, generic_cache)?;
            let ptr_ty = Ty::Pointer(Box::new(inner_typed.inferred_type.clone()));
            Ok(TypedExpr {
                inferred_type: ptr_ty,
                expression: TypedExprKind::AddressOf(Box::new(inner_typed)),
            })
        }
        PExpr::Dereference(inner) => {
            let inner_typed = expr_to_typed(*inner, current_scope, generic_cache)?;
            let pointee_ty = match &inner_typed.inferred_type {
                Ty::Pointer(pointee) => *pointee.clone(),
                other => {
                    return Err(format!(
                        "expr_to_typed: dereference of non-pointer type {:?}",
                        other
                    )
                    .into());
                }
            };
            Ok(TypedExpr {
                inferred_type: pointee_ty,
                expression: TypedExprKind::Deref(Box::new(inner_typed)),
            })
        }
        PExpr::Cast { expr, target_type } => {
            let inner_typed = expr_to_typed(*expr, current_scope, generic_cache)?;
            let typed_target = map_type(target_type)?;
            Ok(TypedExpr {
                inferred_type: typed_target.clone(),
                expression: TypedExprKind::Cast {
                    expr: Box::new(inner_typed),
                    target_type: typed_target,
                },
            })
        }
        PExpr::IndexAccess { object, index } => {
            let obj_typed = expr_to_typed(*object, current_scope, generic_cache)?;
            let idx_typed = expr_to_typed(*index, current_scope, generic_cache)?;
            require_integer_index(&idx_typed.inferred_type, "index access")?;
            let pointee_ty = match &obj_typed.inferred_type {
                Ty::Pointer(inner) => *inner.clone(),
                Ty::Array { element_type, .. } => *element_type.clone(),
                other => {
                    return Err(format!(
                        "expr_to_typed: index access on non-pointer type {:?}",
                        other
                    )
                    .into());
                }
            };
            Ok(TypedExpr {
                inferred_type: pointee_ty,
                expression: TypedExprKind::IndexAccess {
                    object: Box::new(obj_typed),
                    index: Box::new(idx_typed),
                },
            })
        }
        PExpr::ArrayLiteral { elements } => {
            let typed_elements: Vec<TypedExpr> = elements
                .into_iter()
                .map(|e| expr_to_typed(e, current_scope, generic_cache))
                .collect::<Result<_, _>>()?;
            if typed_elements.is_empty() {
                return Err("array literal must have at least one element"
                    .to_string()
                    .into());
            }
            let elem_ty = typed_elements[0].inferred_type.clone();
            // Unify element types with the same numeric coercion rule as
            // `var x: T = ...` (literals relabel, numerics insert `Cast`).
            // This accepts e.g. `[@int(1), @int8(2)]` and `[1, null]`;
            // genuinely incompatible mixes still error below.
            let mut coerced = Vec::with_capacity(typed_elements.len());
            for (i, e) in typed_elements.into_iter().enumerate() {
                if i == 0 {
                    coerced.push(e);
                    continue;
                }
                let found = e.inferred_type.clone();
                coerced.push(coerce_or_alias(e, &elem_ty, current_scope).map_err(|_| {
                    format!(
                        "array literal: element {} has incompatible type {:?}, expected {:?}",
                        i, found, elem_ty
                    )
                })?);
            }
            let typed_elements = coerced;
            let size = typed_elements.len() as u64;
            Ok(TypedExpr {
                inferred_type: Ty::Array {
                    element_type: Box::new(elem_ty),
                    size,
                },
                expression: TypedExprKind::ArrayLiteral {
                    elements: typed_elements,
                },
            })
        }
        PExpr::Unary {
            operator,
            expression,
        } => match operator {
            Operator::Minus => {
                let inner = expr_to_typed(*expression, current_scope, generic_cache)?;
                let is_float = matches!(
                    &inner.inferred_type,
                    Ty::Builtin(b) if is_float_builtin(b)
                );
                let zero = TypedExpr {
                    inferred_type: inner.inferred_type.clone(),
                    expression: if is_float {
                        TypedExprKind::LiteralFloat { value: 0.0 }
                    } else {
                        TypedExprKind::LiteralInt { value: 0 }
                    },
                };
                Ok(TypedExpr {
                    inferred_type: inner.inferred_type.clone(),
                    expression: TypedExprKind::Binary {
                        left: Box::new(zero),
                        operator: BinOp::Sub,
                        right: Box::new(inner),
                    },
                })
            }
            Operator::LogicalNot => expr_to_typed(
                PExpr::Binary {
                    left: expression,
                    operator: Operator::DoubleEquals,
                    right: Box::new(PExpr::Literal(Literal::Boolean(false))),
                },
                current_scope,
                generic_cache,
            ),
            Operator::Tilde => {
                // Desugar ~x to x ^ (-1) which in two's complement flips all bits
                let inner = expr_to_typed(*expression, current_scope, generic_cache)?;
                let minus_one = TypedExpr {
                    inferred_type: inner.inferred_type.clone(),
                    expression: TypedExprKind::LiteralInt { value: u64::MAX },
                };
                Ok(TypedExpr {
                    inferred_type: inner.inferred_type.clone(),
                    expression: TypedExprKind::Binary {
                        left: Box::new(inner),
                        operator: BinOp::Xor,
                        right: Box::new(minus_one),
                    },
                })
            }
            op => Err(format!("unsupported unary operator {:?}", op).into()),
        },
        PExpr::QualifiedAccess { module, member } => {
            let module_alias = module.value.clone();
            let typed_module = current_scope
                .lookup_module(&module_alias)
                .ok_or_else(|| format!("unknown module '{}'", module_alias))?;
            let sym = typed_module
                .exports
                .get(&member)
                .ok_or_else(|| format!("'{}' not found in module '{}'", member, module_alias))?;
            let inferred_type = match sym {
                TypedSymbol::Binding(b) => b.ty.clone(),
                TypedSymbol::Function(f) => Ty::Function {
                    argument_types: f.params.iter().map(|(_, t)| t.clone()).collect(),
                    return_type: Box::new(f.return_type.clone()),
                },
                TypedSymbol::Type(t) => t.clone(),
                TypedSymbol::GenericFunction(_) => {
                    return Err(format!(
                        "'{}::{}' is a generic function and cannot be used as a value",
                        module_alias, member
                    )
                    .into());
                }
            };
            Ok(TypedExpr {
                inferred_type,
                expression: TypedExprKind::QualifiedAccess {
                    module: module_alias,
                    name: member,
                },
            })
        }
    }
}

pub(super) fn process_escape_sequences(raw: &str) -> Result<String, AnalysisError> {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars
            .next()
            .ok_or("trailing backslash in string literal".to_string())?
        {
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            '\\' => out.push('\\'),
            '\'' => out.push('\''),
            '"' => out.push('"'),
            '0' => out.push('\0'),
            'x' => {
                let h1 = chars
                    .next()
                    .ok_or("expected hex digit after \\x".to_string())?;
                let h2 = chars
                    .next()
                    .ok_or("expected two hex digits after \\x".to_string())?;
                let byte = u8::from_str_radix(&format!("{}{}", h1, h2), 16)
                    .map_err(|_| format!("invalid hex escape \\x{}{}", h1, h2))?;
                out.push(byte as char);
            }
            'u' => {
                if chars.next() != Some('{') {
                    return Err("expected '{{' after \\u".to_string().into());
                }
                let mut hex = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some(d) => hex.push(d),
                        None => return Err("unterminated \\u{{...}} escape".to_string().into()),
                    }
                }
                let codepoint = u32::from_str_radix(&hex, 16)
                    .map_err(|_| format!("invalid unicode escape \\u{{{}}}", hex))?;
                let ch = char::from_u32(codepoint)
                    .ok_or_else(|| format!("invalid unicode scalar \\u{{{}}}", hex))?;
                out.push(ch);
            }
            other => return Err(format!("unknown escape sequence \\{}", other).into()),
        }
    }
    Ok(out)
}
