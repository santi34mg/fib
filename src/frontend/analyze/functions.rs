use std::collections::HashMap;

use crate::frontend::ast::{
    function_declaration::FunctionDeclaration, type_expression::TypeExpression,
};
use crate::frontend::identifier::Identifier;
use crate::frontend::tokens::builtin::BuiltinType;
use crate::frontend::typed_ast::{
    GenericFunctionTemplate, ScopeKind, SymbolTable, Ty, TypedBinding, TypedFunction, TypedSymbol,
};

use super::AnalysisError;
use super::generics::is_generic_function;
use super::statements::stmt_to_typed;
use super::types::map_type;

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
    // Only include non-comptime parameters in the Typed function signature.
    let (params, return_type) = signature_to_typed(&function_declaration)?;

    // Enter a function scope holding the runtime parameters. Exited before
    // returning so params never leak into the enclosing (global) table.
    current_scope.enter_scope(ScopeKind::Function);
    for param in &function_declaration.signature.parameters {
        let ty = map_type(param.parameter_type.clone())?;
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
            .filter(|(_, p)| matches!(p.parameter_type, TypeExpression::TypeKeyword))
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
    let (params, return_type) = signature_to_typed(function_declaration)?;
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
