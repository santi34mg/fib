use std::collections::HashMap;

use crate::frontend::ast::{
    function_declaration::FunctionDeclaration, type_expression::TypeExpressionKind,
};
use crate::frontend::identifier::Identifier;
use crate::frontend::tokens::builtin::BuiltinType;
use crate::frontend::typed_ast::{
    GenericFunctionTemplate, ScopeKind, SymbolTable, Ty, TypedBinding, TypedExpr, TypedFunction,
    TypedIf, TypedStatement, TypedSwitchArm, TypedSymbol,
};

use super::AnalysisError;
use super::expressions::check_assignable;
use super::generics::is_generic_function;
use super::statements::stmt_to_typed;
use super::types::map_type;

/// Fallback line for signature errors: the function's first body statement,
/// if any. Signature types carry no spans in the AST, so a bad parameter or
/// return type would otherwise report `line: None`. Body errors already carry
/// precise lines via `stmt_to_typed`, which always win (`with_line` keeps
/// the inner line).
fn fn_body_line(function_declaration: &FunctionDeclaration) -> Option<usize> {
    function_declaration
        .body
        .as_ref()
        .and_then(|b| b.statements.first().map(|s| s.span.line))
}

/// Attach `line` when known, pass the error through otherwise.
fn with_opt_line(err: AnalysisError, line: Option<usize>) -> AnalysisError {
    match line {
        Some(l) => err.with_line(l),
        None => err,
    }
}

/// Map an AST function signature to Typed parameter and return types.
/// Comptime (`type`-typed) parameters are excluded from the parameter list.
pub(super) fn signature_to_typed(
    function_declaration: &FunctionDeclaration,
) -> Result<(Vec<(Identifier, Ty)>, Ty), AnalysisError> {
    let mut params = Vec::new();
    for param in &function_declaration.signature.parameters {
        let ty = map_type(param.parameter_type.clone())?;
        if ty != Ty::Type {
            params.push((param.parameter_name.clone(), ty));
        }
    }
    let return_type = match function_declaration.signature.return_type.clone() {
        Some(rt) => map_type(rt)?,
        None => Ty::Builtin(BuiltinType::Void),
    };
    Ok((params, return_type))
}

pub(super) fn func_to_typed(
    function_declaration: FunctionDeclaration,
    current_scope: &mut SymbolTable,
    generic_cache: &mut HashMap<String, TypedFunction>,
) -> Result<TypedFunction, AnalysisError> {
    // Signature errors have no span of their own; fall back to the first
    // body line so they point at the function instead of nowhere.
    let fn_line = fn_body_line(&function_declaration);
    // Only include non-comptime parameters in the Typed function signature.
    let (params, return_type) =
        signature_to_typed(&function_declaration).map_err(|e| with_opt_line(e, fn_line))?;

    // Enter a function scope holding the runtime parameters. Exited before
    // returning so params never leak into the enclosing (global) table.
    current_scope.enter_scope(ScopeKind::Function);
    for param in &function_declaration.signature.parameters {
        let ty = map_type(param.parameter_type.clone()).map_err(|e| with_opt_line(e, fn_line))?;
        if ty != Ty::Type {
            current_scope.insert(
                param.parameter_name.clone(),
                TypedSymbol::Binding(TypedBinding {
                    name: param.parameter_name.clone(),
                    ty,
                    init: None,
                    mutable: true,
                }),
            );
        }
    }

    let mut body = Vec::new();
    if let Some(fb) = function_declaration.body {
        for stmt in fb.statements {
            body.push(stmt_to_typed(stmt, current_scope, generic_cache)?);
        }
    }
    validate_return_types(&body, &return_type, current_scope).map_err(|e| {
        // Body errors already carry precise lines via `stmt_to_typed`.
        with_opt_line(e, fn_line)
    })?;
    current_scope.exit_scope();
    Ok(TypedFunction {
        name: function_declaration.signature.name,
        params,
        return_type,
        body,
        is_extern: function_declaration.is_extern,
        is_variadic: function_declaration.is_variadic,
    })
}

/// Check every `return` in `body` against the declared return type, using the
/// same `check_assignable` rule as assignments so strictness is identical.
/// Multi-value returns (`return a, b`) must match a tuple return type element
/// by element; a single value must coerce to the return type. `return` with no
/// value is only legal in `@void` functions.
fn validate_return_types(
    body: &[TypedStatement],
    return_type: &Ty,
    scope: &SymbolTable,
) -> Result<(), AnalysisError> {
    fn check_one(
        return_type: &Ty,
        value: &TypedExpr,
        scope: &SymbolTable,
    ) -> Result<(), AnalysisError> {
        let found = value.inferred_type.clone();
        check_assignable(
            return_type,
            value.clone(),
            scope,
            &format!("return value of type {:?}", return_type),
        )
        .map(|_| ())
        .map_err(|_| {
            format!(
                "return type mismatch: function returns {:?} but this returns {:?}",
                return_type, found
            )
            .into()
        })
    }

    for stmt in body {
        match stmt {
            TypedStatement::Return(Some(ret)) => {
                if ret.values.is_empty() {
                    if *return_type != Ty::Builtin(BuiltinType::Void) {
                        return Err(format!(
                            "return type mismatch: function returns {:?} but `return` has no value",
                            return_type
                        )
                        .into());
                    }
                    continue;
                }
                match return_type {
                    Ty::Tuple { elements } => {
                        if ret.values.len() != elements.len() {
                            return Err(format!(
                                "return type mismatch: function returns {} value(s) but this returns {}",
                                elements.len(),
                                ret.values.len()
                            )
                            .into());
                        }
                        for (value, elem_ty) in ret.values.iter().zip(elements.iter()) {
                            check_one(elem_ty, value, scope)?;
                        }
                    }
                    single => {
                        if ret.values.len() != 1 {
                            return Err(format!(
                                "return type mismatch: function returns {:?} but this returns {} values",
                                single,
                                ret.values.len()
                            )
                            .into());
                        }
                        check_one(single, &ret.values[0], scope)?;
                    }
                }
            }
            TypedStatement::Return(None) => {
                if *return_type != Ty::Builtin(BuiltinType::Void) {
                    return Err(format!(
                        "return type mismatch: function returns {:?} but `return` has no value",
                        return_type
                    )
                    .into());
                }
            }
            TypedStatement::If(TypedIf {
                then_branch,
                else_branch,
                ..
            }) => {
                validate_return_types(then_branch, return_type, scope)?;
                if let Some(else_branch) = else_branch {
                    validate_return_types(else_branch, return_type, scope)?;
                }
            }
            TypedStatement::For {
                init, post, body, ..
            } => {
                if let Some(init) = init {
                    validate_return_types(std::slice::from_ref(init.as_ref()), return_type, scope)?;
                }
                if let Some(post) = post {
                    validate_return_types(std::slice::from_ref(post.as_ref()), return_type, scope)?;
                }
                validate_return_types(body, return_type, scope)?;
            }
            TypedStatement::Defer(inner) => {
                validate_return_types(std::slice::from_ref(inner.as_ref()), return_type, scope)?;
            }
            TypedStatement::Switch { arms, .. } => {
                for TypedSwitchArm { body, .. } in arms {
                    validate_return_types(body, return_type, scope)?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

pub(super) fn resolve_function_decl(
    function_declaration: &FunctionDeclaration,
    current_scope: &mut SymbolTable,
) -> Result<(), AnalysisError> {
    // Generic functions (those with `type`-typed parameters) are stored as templates.
    // They are instantiated lazily when called with concrete type arguments.
    if is_generic_function(function_declaration) {
        let comptime_params: Vec<usize> = function_declaration
            .signature
            .parameters
            .iter()
            .enumerate()
            .filter(|(_, p)| matches!(&p.parameter_type.kind, TypeExpressionKind::TypeKeyword))
            .map(|(i, _)| i)
            .collect();
        current_scope.insert(
            function_declaration.signature.name.clone(),
            TypedSymbol::GenericFunction(GenericFunctionTemplate {
                name: function_declaration.signature.name.clone(),
                ast_decl: function_declaration.clone(),
                comptime_params,
            }),
        );
        return Ok(());
    }

    // Register the signature only. The body is analyzed exactly once, in
    // `analyze()`, *after* this symbol is in scope — which is what makes
    // recursive calls resolve.
    let fn_line = fn_body_line(function_declaration);
    let (params, return_type) =
        signature_to_typed(function_declaration).map_err(|e| with_opt_line(e, fn_line))?;
    current_scope.insert(
        function_declaration.signature.name.clone(),
        TypedSymbol::Function(TypedFunction {
            name: function_declaration.signature.name.clone(),
            params,
            return_type,
            body: Vec::new(),
            is_extern: function_declaration.is_extern,
            is_variadic: function_declaration.is_variadic,
        }),
    );
    Ok(())
}
