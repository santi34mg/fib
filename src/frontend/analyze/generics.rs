use std::collections::HashMap;

use crate::frontend::ast::{
    expression::{Expression as PExpr, ExpressionKind as PExprKind},
    field::Field,
    function_declaration::FunctionDeclaration,
    statement::StatementKind,
    type_expression::{TypeExpression, TypeExpressionKind},
};
use crate::frontend::identifier::Identifier;
use crate::frontend::typed_ast::{
    GenericFunctionTemplate, SymbolTable, Ty, TypedFunction, TypedSymbol,
};

use super::AnalysisError;
use super::functions::{func_to_typed, signature_to_typed};

/// Returns true if the function has at least one `type`-typed parameter (making it generic).
pub(super) fn is_generic_function(fn_decl: &FunctionDeclaration) -> bool {
    fn_decl
        .signature
        .parameters
        .iter()
        .any(|p| matches!(&p.parameter_type.kind, TypeExpressionKind::TypeKeyword))
}

/// Compute a stable mangle string for a type expression (used in generic function name mangling).
pub(super) fn mangle_type_expr(te: &TypeExpression) -> String {
    match &te.kind {
        TypeExpressionKind::Builtin(bt) => format!("{}", bt),
        TypeExpressionKind::Identifier(id) => id.value.clone(),
        TypeExpressionKind::Pointer { pointed_type, .. } => {
            format!("ptr_{}", mangle_type_expr(pointed_type))
        }
        TypeExpressionKind::Array { element_type, size } => {
            format!("arr{}_{}", size, mangle_type_expr(element_type))
        }
        TypeExpressionKind::Slice { element_type } => {
            format!("slice_{}", mangle_type_expr(element_type))
        }
        TypeExpressionKind::Struct { .. } => "struct".to_string(),
        TypeExpressionKind::Enum { .. } => "enum".to_string(),
        TypeExpressionKind::Function { .. } => "fn".to_string(),
        TypeExpressionKind::Tuple { elements } => format!(
            "tuple_{}",
            elements
                .iter()
                .map(mangle_type_expr)
                .collect::<Vec<_>>()
                .join("_")
        ),
        TypeExpressionKind::QualifiedIdentifier { module, name } => {
            format!("{}__{}", module.value, name.value)
        }
        TypeExpressionKind::TypeKeyword => "type".to_string(),
    }
}

/// Substitute all occurrences of type identifiers in `subs` within a TypeExpression.
pub(super) fn substitute_type(
    te: &TypeExpression,
    subs: &HashMap<String, TypeExpression>,
) -> TypeExpression {
    let span = te.span;
    match &te.kind {
        TypeExpressionKind::Identifier(id) => {
            if let Some(replacement) = subs.get(&id.value) {
                replacement.clone()
            } else {
                te.clone()
            }
        }
        TypeExpressionKind::Pointer { pointed_type } => TypeExpression::at(
            TypeExpressionKind::Pointer {
                pointed_type: Box::new(substitute_type(pointed_type, subs)),
            },
            span,
        ),
        TypeExpressionKind::Array { element_type, size } => TypeExpression::at(
            TypeExpressionKind::Array {
                element_type: Box::new(substitute_type(element_type, subs)),
                size: *size,
            },
            span,
        ),
        TypeExpressionKind::Slice { element_type } => TypeExpression::at(
            TypeExpressionKind::Slice {
                element_type: Box::new(substitute_type(element_type, subs)),
            },
            span,
        ),
        TypeExpressionKind::Struct { fields } => TypeExpression::at(
            TypeExpressionKind::Struct {
                fields: fields
                    .iter()
                    .map(|f| Field {
                        label: f.label.clone(),
                        type_id: substitute_type(&f.type_id, subs),
                    })
                    .collect(),
            },
            span,
        ),
        TypeExpressionKind::Function {
            argument_types,
            return_type,
        } => TypeExpression::at(
            TypeExpressionKind::Function {
                argument_types: argument_types
                    .iter()
                    .map(|t| substitute_type(t, subs))
                    .collect(),
                return_type: Box::new(substitute_type(return_type, subs)),
            },
            span,
        ),
        TypeExpressionKind::Tuple { elements } => TypeExpression::at(
            TypeExpressionKind::Tuple {
                elements: elements.iter().map(|t| substitute_type(t, subs)).collect(),
            },
            span,
        ),
        // Builtins, QualifiedIdentifier, TypeKeyword contain no substitutable identifiers
        _ => te.clone(),
    }
}

pub(super) fn substitute_in_expr(expr: &mut PExpr, subs: &HashMap<String, TypeExpression>) {
    match &mut expr.kind {
        PExprKind::Cast {
            target_type,
            expr: inner,
        } => {
            *target_type = substitute_type(target_type, subs);
            substitute_in_expr(inner, subs);
        }
        PExprKind::Binary { left, right, .. } => {
            substitute_in_expr(left, subs);
            substitute_in_expr(right, subs);
        }
        PExprKind::Unary { expression, .. } => substitute_in_expr(expression, subs),
        PExprKind::Call { callee, args } => {
            substitute_in_expr(callee, subs);
            for arg in args {
                substitute_in_expr(arg, subs);
            }
        }
        PExprKind::FieldAccess { object, .. } => substitute_in_expr(object, subs),
        PExprKind::AddressOf(inner)
        | PExprKind::Dereference(inner)
        | PExprKind::Grouping(inner) => {
            substitute_in_expr(inner, subs);
        }
        PExprKind::IndexAccess { object, index } => {
            substitute_in_expr(object, subs);
            substitute_in_expr(index, subs);
        }
        PExprKind::ArrayLiteral { elements } => {
            for e in elements {
                substitute_in_expr(e, subs);
            }
        }
        PExprKind::StructConstruct { fields, .. } => {
            for (_, val) in fields {
                substitute_in_expr(val, subs);
            }
        }
        // Literals, Identifiers, QualifiedAccess, TypeValue — no substitution
        _ => {}
    }
}

pub(super) fn substitute_in_stmt(stmt: &mut StatementKind, subs: &HashMap<String, TypeExpression>) {
    match stmt {
        StatementKind::VariableDeclaration(decl) => {
            if let Some(t) = &decl.constant_type {
                decl.constant_type = Some(substitute_type(t, subs));
            }
            if let Some(e) = &mut decl.expression {
                substitute_in_expr(e, subs);
            }
        }
        StatementKind::Return(Some(exprs)) => {
            for expr in exprs {
                substitute_in_expr(expr, subs);
            }
        }
        StatementKind::ExpressionStatement(expr) => substitute_in_expr(expr, subs),
        StatementKind::Assignment { expr, .. } => substitute_in_expr(expr, subs),
        StatementKind::MultiAssignment { targets, values } => {
            for target in targets {
                substitute_in_expr(target, subs);
            }
            for value in values {
                substitute_in_expr(value, subs);
            }
        }
        StatementKind::MultiVariableDeclaration { values, .. } => {
            for value in values {
                substitute_in_expr(value, subs);
            }
        }
        StatementKind::FieldAssign { expr, .. } => substitute_in_expr(expr, subs),
        StatementKind::DerefAssign { pointer, expr } => {
            substitute_in_expr(pointer, subs);
            substitute_in_expr(expr, subs);
        }
        StatementKind::IndexAssign {
            object,
            index,
            expr,
        } => {
            substitute_in_expr(object, subs);
            substitute_in_expr(index, subs);
            substitute_in_expr(expr, subs);
        }
        StatementKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            substitute_in_expr(condition, subs);
            for s in then_branch {
                substitute_in_stmt(&mut s.kind, subs);
            }
            if let Some(eb) = else_branch {
                for s in eb {
                    substitute_in_stmt(&mut s.kind, subs);
                }
            }
        }
        StatementKind::While { condition, body } => {
            if let Some(cond) = condition {
                substitute_in_expr(cond, subs);
            }
            for s in body {
                substitute_in_stmt(&mut s.kind, subs);
            }
        }
        StatementKind::Defer(inner) => substitute_in_stmt(&mut inner.kind, subs),
        StatementKind::Switch { subject, arms } => {
            substitute_in_expr(subject, subs);
            for arm in arms {
                for s in &mut arm.body {
                    substitute_in_stmt(&mut s.kind, subs);
                }
            }
        }
        StatementKind::Break | StatementKind::Continue | StatementKind::Return(None) => {}
    }
}

/// Monomorphize a generic function with the given type arguments.
/// Returns the mangled name and the instantiated return type.
/// The instantiated TypedFunction is stored in `generic_cache`.
pub(super) fn instantiate_generic(
    template: &GenericFunctionTemplate,
    call_args: &[PExpr],
    scope: &SymbolTable,
    generic_cache: &mut HashMap<String, TypedFunction>,
) -> Result<(String, Ty), AnalysisError> {
    // Build type substitution map: param_name -> TypeExpression
    let mut subs: HashMap<String, TypeExpression> = HashMap::new();
    let mut type_arg_names: Vec<String> = Vec::new();

    for &param_idx in &template.comptime_params {
        let param_name = template.ast_decl.signature.parameters[param_idx]
            .parameter_name
            .value
            .clone();
        let arg = call_args.get(param_idx).ok_or_else(|| {
            format!(
                "generic call to '{}': missing argument for comptime param '{}'",
                template.name, param_name
            )
        })?;
        let type_expr = match &arg.kind {
            PExprKind::TypeValue(te) => te.clone(),
            PExprKind::Identifier(id) => {
                // A user-defined type name passed as argument
                match scope.lookup(id) {
                    Some(TypedSymbol::Type(_)) => TypeExpression::at(
                        TypeExpressionKind::Identifier(id.clone()),
                        arg.span,
                    ),
                    _ => return Err(format!(
                        "generic call to '{}': argument for comptime param '{}' must be a type, got identifier '{}'",
                        template.name, param_name, id
                    ).into()),
                }
            }
            _ => return Err(format!(
                "generic call to '{}': argument for comptime param '{}' must be a type expression",
                template.name, param_name
            )
            .into()),
        };
        type_arg_names.push(mangle_type_expr(&type_expr));
        subs.insert(param_name, type_expr);
    }

    // Build mangled name: funcname__type1__type2
    let mangled = format!("{}__{}", template.name.value, type_arg_names.join("__"));

    // Check cache — deduplication
    if let Some(cached) = generic_cache.get(&mangled) {
        return Ok((mangled, cached.return_type.clone()));
    }

    // Clone and substitute the AST function declaration
    let mut substituted = template.ast_decl.clone();
    substituted.signature.name = Identifier {
        value: mangled.clone(),
    };

    // Apply type substitution to parameter types (skip comptime params — they will be removed)
    for param in &mut substituted.signature.parameters {
        param.parameter_type = substitute_type(&param.parameter_type, &subs);
    }
    // Remove comptime parameters from the signature
    let runtime_params: Vec<_> = substituted
        .signature
        .parameters
        .into_iter()
        .enumerate()
        .filter(|(i, _)| !template.comptime_params.contains(i))
        .map(|(_, p)| p)
        .collect();
    substituted.signature.parameters = runtime_params;

    // Substitute return type
    if let Some(rt) = &substituted.signature.return_type {
        substituted.signature.return_type = Some(substitute_type(rt, &subs));
    }
    // Substitute in body statements
    if let Some(ref mut body) = substituted.body {
        for stmt in &mut body.statements {
            substitute_in_stmt(&mut stmt.kind, &subs);
        }
    }

    // Pre-insert a signature-only placeholder so a recursive call inside the
    // body hits the cache instead of re-instantiating forever. It is replaced
    // with the fully analyzed function below.
    let (placeholder_params, placeholder_ret) = signature_to_typed(&substituted)?;
    generic_cache.insert(
        mangled.clone(),
        TypedFunction {
            name: substituted.signature.name.clone(),
            params: placeholder_params,
            return_type: placeholder_ret,
            body: Vec::new(),
            is_extern: substituted.is_extern,
            is_variadic: substituted.is_variadic,
        },
    );

    // Analyze the substituted function
    let mut func_scope = scope.clone();
    let typed_func = func_to_typed(substituted, &mut func_scope, generic_cache)?;
    let return_type = typed_func.return_type.clone();

    // Cache the instantiation
    generic_cache.insert(mangled.clone(), typed_func);

    Ok((mangled, return_type))
}
