use std::collections::HashMap;
use std::fmt;

use crate::frontend::ast::{Ast, declaration::DeclarationNode, type_expression::TypeExpression};
use crate::frontend::typed_ast::{
    SymbolTable, TypedDecl, TypedFunction, TypedModule, TypedProgram, TypedTypeDecl,
};

use functions::func_to_typed;
use generics::is_generic_function;
use types::{map_type, resolve_declaration};

mod expressions;
mod functions;
mod generics;
mod statements;
mod test;
mod types;
#[derive(Debug)]
pub struct AnalysisError {
    pub msg: String,
    /// The source line the error originates from, if known. Filled in as the
    /// error propagates up through the statement that triggered it.
    pub line: Option<usize>,
}

impl std::error::Error for AnalysisError {}

impl fmt::Display for AnalysisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(f, "AnalysisError at line {}: {}", line, self.msg),
            None => write!(f, "AnalysisError: {}", self.msg),
        }
    }
}

impl From<String> for AnalysisError {
    fn from(value: String) -> Self {
        Self {
            msg: value,
            line: None,
        }
    }
}

/// Perform semantic analysis on the parser AST and produce a vector of Typed functions.
pub fn analyze(
    ast: Ast,
    resolved_modules: &std::collections::HashMap<Vec<String>, TypedModule>,
) -> Result<TypedProgram, AnalysisError> {
    // name resolution
    let mut current_scope = SymbolTable::new();

    // Import resolution: process all import declarations first so that module
    // symbols are available when resolving subsequent declarations.
    for declaration in &ast.declarations {
        if let DeclarationNode::ImportDeclaration(import) = declaration {
            let path_strs: Vec<String> = import.path.iter().map(|id| id.value.clone()).collect();
            let module = resolved_modules
                .get(&path_strs)
                .ok_or_else(|| format!("module '{}' not found", path_strs.join("::")))?;

            if let Some(selected) = &import.selective {
                // Copy selected symbols directly into local scope
                for name in selected {
                    let sym = module.exports.get(name).ok_or_else(|| {
                        format!("'{}' not found in module '{}'", name, path_strs.join("::"))
                    })?;
                    current_scope.insert(name.clone(), sym.clone());
                }
            } else {
                // Register as a named module in scope
                let local_name = import
                    .alias
                    .as_ref()
                    .map(|a| a.value.clone())
                    .unwrap_or_else(|| import.path.last().unwrap().value.clone());
                current_scope.insert_module(local_name, module.clone());
            }
        }
    }

    // Collect imported declarations for lowering
    let mut imported_declarations: Vec<TypedDecl> = Vec::new();
    for module in current_scope.modules() {
        imported_declarations.extend(module.declarations.clone());
    }
    // Also collect selectively imported declarations
    for declaration in &ast.declarations {
        if let DeclarationNode::ImportDeclaration(import) = declaration
            && import.selective.is_some()
        {
            let path_strs: Vec<String> = import.path.iter().map(|id| id.value.clone()).collect();
            if let Some(module) = resolved_modules.get(&path_strs) {
                imported_declarations.extend(module.declarations.clone());
            }
        }
    }

    let mut typed_declarations: Vec<TypedDecl> = Vec::new();
    // Cache of instantiated generic functions keyed by mangled name.
    let mut generic_cache: HashMap<String, TypedFunction> = HashMap::new();

    for declaration in ast.declarations {
        resolve_declaration(&declaration, &mut current_scope)?;
        let typed_declaration: Option<TypedDecl> = match declaration {
            DeclarationNode::ImportDeclaration(_) => None,
            DeclarationNode::FunctionDeclaration(function_declaration) => {
                // Generic functions are stored as templates; they produce no direct Typed declaration.
                if is_generic_function(&function_declaration) {
                    None
                } else {
                    Some(TypedDecl::Function(func_to_typed(
                        function_declaration,
                        &mut current_scope,
                        &mut generic_cache,
                    )?))
                }
            }
            DeclarationNode::TypeDeclaration(ty_decl) => match ty_decl.expression {
                TypeExpression::TypeKeyword => None,
                expr => {
                    let ty = map_type(expr)?;
                    Some(TypedDecl::Type(TypedTypeDecl {
                        name: ty_decl.name,
                        ty,
                    }))
                }
            },
        };
        if let Some(typed_declaration) = typed_declaration {
            typed_declarations.push(typed_declaration);
        }
    }

    // Collect generic instantiations produced during analysis. Sort by
    // mangled name so the emitted declaration order is deterministic.
    let mut generic_fns: Vec<_> = generic_cache.into_iter().collect();
    generic_fns.sort_by(|a, b| a.0.cmp(&b.0));
    for (_, func) in generic_fns {
        typed_declarations.push(TypedDecl::Function(func));
    }

    let compilation_unit = TypedProgram {
        symbol_table: current_scope,
        declarations: typed_declarations,
        imported_declarations,
    };
    Ok::<TypedProgram, AnalysisError>(compilation_unit)
}
