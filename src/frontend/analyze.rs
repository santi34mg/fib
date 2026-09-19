use std::collections::HashMap;
use std::fmt;

use crate::diagnostics::{ErrorKind, Span};
use crate::frontend::ast::{
    Ast, declaration::DeclarationNode, type_expression::TypeExpressionKind,
};
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
#[derive(Debug, Clone)]
pub struct AnalysisError {
    pub kind: ErrorKind,
    /// The source position the error originates from, if known. Filled in as
    /// the error propagates up through the statement that triggered it.
    pub span: Option<Span>,
    pub message: String,
    /// An optional remedy hint rendered alongside the diagnostic.
    pub hint: Option<String>,
}

impl std::error::Error for AnalysisError {}

impl fmt::Display for AnalysisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.span {
            Some(span) => write!(f, "AnalysisError at line {}: {}", span.line, self.message),
            None => write!(f, "AnalysisError: {}", self.message),
        }
    }
}

impl From<String> for AnalysisError {
    fn from(value: String) -> Self {
        Self {
            kind: ErrorKind::Type,
            span: None,
            message: value,
            hint: None,
        }
    }
}

impl From<&str> for AnalysisError {
    fn from(value: &str) -> Self {
        Self {
            kind: ErrorKind::Type,
            span: None,
            message: value.to_string(),
            hint: None,
        }
    }
}

impl AnalysisError {
    /// Attach a source line if none is set yet. Inner errors are more
    /// precise than outer context, so an existing span always wins.
    /// Use this instead of bare `format!(...)?` wherever a line is known.
    pub fn with_line(mut self, line: usize) -> Self {
        if self.span.is_none() {
            self.span = Some(Span::new(line, 1));
        }
        self
    }

    /// Attach a precise source span. Overwrites any coarser line, since a
    /// caller that has column information is closer to the offending node.
    pub fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }

    /// Attach a source span only when none is set yet. Used when bubbling an
    /// error up through enclosing nodes: the innermost node (which pinpointed
    /// the problem) keeps its precise span, while this only fills gaps.
    pub fn with_span_fallback(mut self, span: Span) -> Self {
        if self.span.is_none() {
            self.span = Some(span);
        }
        self
    }

    /// Label the error with the stage that produced it.
    pub fn with_kind(mut self, kind: ErrorKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

/// Best-effort source line for a top-level declaration.
///
/// Function bodies carry statement lines, so signature/resolve errors can
/// fall back to the first body statement. Imports and type aliases carry no
/// line in the AST today (needs a parser change, out of scope here), hence
/// `None` — callers still route through `with_opt_line` so a future line
/// field plugs in with a one-line change.
fn declaration_line(declaration: &DeclarationNode) -> Option<usize> {
    match declaration {
        DeclarationNode::FunctionDeclaration(f) => f
            .body
            .as_ref()
            .and_then(|b| b.statements.first().map(|s| s.span.line)),
        DeclarationNode::ImportDeclaration(_) | DeclarationNode::TypeDeclaration(_) => None,
    }
}

/// Attach `line` when known, pass the error through otherwise.
/// Keeps the `with_line` pattern uniform at sites (imports, signatures)
/// where the AST carries no line yet, instead of bare `format!(...)?`.
fn with_opt_line(err: AnalysisError, line: Option<usize>) -> AnalysisError {
    match line {
        Some(l) => err.with_line(l),
        None => err,
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
    // Import declarations carry no line in the AST, so these errors stay
    // line-less until the parser records declaration spans (out of scope).
    // They still route through `with_opt_line` for uniformity.
    for declaration in &ast.declarations {
        if let DeclarationNode::ImportDeclaration(import) = declaration {
            let import_line = declaration_line(declaration);
            let path_strs: Vec<String> = import.path.iter().map(|id| id.value.clone()).collect();
            let module = resolved_modules.get(&path_strs).ok_or_else(|| {
                with_opt_line(
                    AnalysisError::from(format!("module '{}' not found", path_strs.join("::"))),
                    import_line,
                )
            })?;

            if let Some(selected) = &import.selective {
                // Copy selected symbols directly into local scope
                for name in selected {
                    let sym = module.exports.get(name).ok_or_else(|| {
                        with_opt_line(
                            AnalysisError::from(format!(
                                "'{}' not found in module '{}'",
                                name,
                                path_strs.join("::")
                            )),
                            import_line,
                        )
                    })?;
                    current_scope.insert(name.clone(), sym.clone());
                }
            } else {
                // Register as a named module in scope
                let local_name = match &import.alias {
                    Some(a) => a.value.clone(),
                    None => import
                        .path
                        .last()
                        .map(|id| id.value.clone())
                        .ok_or_else(|| {
                            with_opt_line(
                                AnalysisError::from("empty import path".to_string()),
                                import_line,
                            )
                        })?,
                };
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
        // `resolve_declaration` (signatures) and `func_to_typed`/`map_type`
        // produce line-less errors; fall back to the declaration's first body
        // line. Precise inner lines (statement-level via `stmt_to_typed`)
        // always win through `with_line`'s get_or_insert.
        let decl_line = declaration_line(&declaration);
        resolve_declaration(&declaration, &mut current_scope)
            .map_err(|e| with_opt_line(e, decl_line))?;
        let typed_declaration: Option<TypedDecl> = match declaration {
            DeclarationNode::ImportDeclaration(_) => None,
            DeclarationNode::FunctionDeclaration(function_declaration) => {
                // Generic functions are stored as templates; they produce no direct Typed declaration.
                if is_generic_function(&function_declaration) {
                    None
                } else {
                    // Same fallback as `declaration_line`, without cloning:
                    // first body statement line, if any.
                    let fn_line = function_declaration
                        .body
                        .as_ref()
                        .and_then(|b| b.statements.first().map(|s| s.span.line));
                    Some(TypedDecl::Function(
                        func_to_typed(function_declaration, &mut current_scope, &mut generic_cache)
                            .map_err(|e| with_opt_line(e, fn_line))?,
                    ))
                }
            }
            DeclarationNode::TypeDeclaration(ty_decl) => {
                if matches!(&ty_decl.expression.kind, TypeExpressionKind::TypeKeyword) {
                    None
                } else {
                    let ty =
                        map_type(ty_decl.expression).map_err(|e| with_opt_line(e, decl_line))?;
                    Some(TypedDecl::Type(TypedTypeDecl {
                        name: ty_decl.name,
                        ty,
                    }))
                }
            }
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
