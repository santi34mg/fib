//! Staged name/type analysis enriches parsed source units in place.
//! Names, types, members and explicit metatype calls resolve. Initialization
//! and structured flow validation remain unfinished.
//! The only lowering boundary is `AnalyzedProject` to `Ir`; analysis never
//! manufactures a separate typed AST or emits IR.

use std::collections::{HashMap, HashSet};

use anyhow::Context;

use crate::build_plan::{BuildPlan, FileId};
use crate::diagnostics::{CompilerError, Span};
use crate::driver::CompilationUnit;
use crate::frontend::analysis_annotation::{BindingAnnotation, BindingId};
use crate::frontend::ast::statement::StatementKind;
use crate::frontend::ast::{
    expression::ExpressionKind, qualified_name::QualifiedIdentifier,
    variable_declaration::VariableDeclaration,
};
use crate::frontend::namespace::{
    DeclId, Declaration, DeclarationKind, LocalId, Namespace, Visibility, name_error,
};
use crate::frontend::scope_kind::ScopeKind;
use crate::frontend::{
    namespace::ProjectSymbols, semantic_type::SemanticTypes, symbol_table::SymbolTable,
};

/// The original ASTs plus project-owned semantic identities and types.
/// AST nodes carry initially pending `analysis_annotation` slots. Successful
/// analysis must leave every applicable slot resolved; public fields alone
/// do not certify that the completion-validation pass has run.
#[derive(Debug, Clone)]
pub struct AnalyzedProject {
    pub units: Vec<CompilationUnit>,
    pub symbols: ProjectSymbols,
    pub types: SemanticTypes,
}

/// Name-pass state for one file. File namespaces are shared, never copied into
/// lexical scopes. Locals take precedence over unqualified file declarations;
/// qualified file selection bypasses that local binding. Labels are separate.
pub struct ResolveContext<'a> {
    pub file_id: FileId,
    pub lexical: SymbolTable,
    pub symbols: &'a ProjectSymbols,
    pub local_ids: &'a mut LocalIdAllocator,
}

impl ResolveContext<'_> {
    pub(super) fn scoped<T>(
        &mut self,
        kind: ScopeKind,
        action: impl FnOnce(&mut Self) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        self.lexical.enter_scope(kind);
        let result = action(self);
        self.lexical.exit_scope()?;
        result
    }
}

/// Name annotations are inputs to typing; type/member lookup cannot insert
/// lexical bindings. The shared table will also record parameter/local types.
pub struct TypeContext<'a> {
    pub file: FileId,
    pub symbols: &'a ProjectSymbols,
    pub types: &'a mut SemanticTypes,
    state: typing::TypeState,
}

impl<'a> TypeContext<'a> {
    pub fn new(file: FileId, symbols: &'a ProjectSymbols, types: &'a mut SemanticTypes) -> Self {
        Self {
            file,
            symbols,
            types,
            state: typing::TypeState::default(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct LocalIdAllocator {
    next: u32,
}

impl LocalIdAllocator {
    pub fn allocate(&mut self) -> anyhow::Result<LocalId> {
        let next = self
            .next
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("local ID space exhausted"))?;

        let id = LocalId(self.next);
        self.next = next;
        Ok(id)
    }
}

#[derive(Clone)]
pub struct Analyzer {
    units: Vec<CompilationUnit>,
    symbols: ProjectSymbols,
    local_ids: LocalIdAllocator,
    paths: HashMap<FileId, std::path::PathBuf>,
    phase: NamePhase,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NamePhase {
    New,
    Collecting,
    Collected,
    Resolving,
    Resolved,
    Typing,
    Typed,
}

impl Analyzer {
    /// Validate that compilation-unit IDs match exactly the named build-plan files,
    /// then construct ProjectSymbols. Do not discover additional files.
    pub fn new(plan: BuildPlan, units: Vec<CompilationUnit>) -> anyhow::Result<Self> {
        let symbols = ProjectSymbols::from_plan(&plan)?;
        let plan_set = plan.files.iter().map(|f| f.id).collect::<HashSet<_>>();
        let unit_set = units
            .iter()
            .map(|u| u.source_file_id)
            .collect::<HashSet<_>>();
        if unit_set.len() != units.len() {
            return Err(name_error("duplicate file IDs in compilation units"));
        }
        if plan_set != unit_set {
            return Err(name_error(
                "compilation-unit IDs do not match build-plan file IDs",
            ));
        }
        Ok(Self {
            units,
            symbols,
            local_ids: LocalIdAllocator::default(),
            paths: plan.files.into_iter().map(|f| (f.id, f.path)).collect(),
            phase: NamePhase::New,
        })
    }

    pub fn analyze(mut self) -> anyhow::Result<AnalyzedProject> {
        // Planned order: collect declarations, resolve names/aliases, resolve
        // types/members, validate initialization and annotations, then hand off.
        self.collect_declarations()?;
        self.resolve_names()?;
        let types = self.resolve_types()?;
        let project = AnalyzedProject {
            units: self.units,
            symbols: self.symbols,
            types,
        };
        validate_analysis(&project)?;
        Ok(project)
    }

    /// Predeclare file bindings for forward references; do not resolve uses or
    /// confuse name visibility with top-level initialization order.
    /// Rejects invalid top-level statements.
    pub fn collect_declarations(&mut self) -> anyhow::Result<()> {
        if self.phase != NamePhase::New {
            return Err(name_error("declaration collection has already started"));
        }
        self.phase = NamePhase::Collecting;
        for unit in &mut self.units {
            let file_id = unit.source_file_id;
            let result = (|| {
                let mut namespace = Namespace {
                    file_id,
                    declarations: Vec::new(),
                    names: HashMap::new(),
                };
                for statement in &mut unit.ast.statements {
                    let span = statement.span;
                    match &mut statement.kind {
                        StatementKind::VariableDeclaration(var) => {
                            let inline = inline_type(var.expression.as_ref());
                            let computed = computed_type(var.expression.as_ref());
                            collect_variable(&mut namespace, var, span, inline, computed)?;
                        }
                        StatementKind::FunctionDeclaration(fun) => {
                            fun.signature.annotation.binding =
                                Some(BindingId::Declaration(add_declaration(
                                    &mut namespace,
                                    &fun.signature.identifier,
                                    fun.is_pub,
                                    DeclarationKind::Value,
                                    span,
                                    true,
                                )?));
                        }
                        StatementKind::MultiVariableDeclaration { bindings, values } => {
                            for (i, binding) in bindings.iter_mut().enumerate() {
                                collect_variable(
                                    &mut namespace,
                                    binding,
                                    span,
                                    inline_type(values.get(i)),
                                    computed_type(values.get(i)),
                                )?;
                            }
                        }
                        _ => {
                            return Err(at_span(
                                name_error("unexpected top-level statement"),
                                span,
                            ));
                        }
                    }
                }
                // Reserve identities without adding lexical names or resolving initializers.
                struct LocalTypes<'a>(&'a mut Namespace);
                impl walk::Visitor for LocalTypes<'_> {
                    fn variable(
                        &mut self,
                        var: &mut VariableDeclaration,
                        span: Span,
                        file: bool,
                        inline: bool,
                    ) -> anyhow::Result<()> {
                        if !file && var.is_comp && inline {
                            var.annotation.binding = Some(BindingId::Declaration(add_declaration(
                                self.0,
                                &var.identifier,
                                false,
                                DeclarationKind::Type,
                                span,
                                false,
                            )?));
                        }
                        Ok(())
                    }
                }
                walk::statements(
                    &mut LocalTypes(&mut namespace),
                    &mut unit.ast.statements,
                    true,
                )?;
                self.symbols.namespaces.insert(file_id, namespace);
                Ok(())
            })();
            result.map_err(|e| with_path(e, self.paths.get(&file_id)))?;
        }
        self.phase = NamePhase::Collected;
        Ok(())
    }

    pub fn resolve_names(&mut self) -> anyhow::Result<()> {
        if self.phase != NamePhase::Collected {
            return Err(name_error(
                "resolve_names requires declaration collection and may only run once",
            ));
        }
        self.phase = NamePhase::Resolving;
        let aliases = aliases::definitions(&self.units, &self.paths)?;
        aliases::resolve(&mut self.symbols, &aliases, &self.paths)?;
        for unit in &mut self.units {
            let file = unit.source_file_id;
            let mut context = ResolveContext {
                file_id: file,
                lexical: SymbolTable::new(),
                local_ids: &mut self.local_ids,
                symbols: &self.symbols,
            };
            unit.ast
                .resolve_name(&mut context)
                .map_err(|e| with_path(e, self.paths.get(&file)))?;
            validate_names(&mut unit.ast).map_err(|e| with_path(e, self.paths.get(&file)))?;
        }
        self.phase = NamePhase::Resolved;
        Ok(())
    }
    /// Resolve and validate semantic annotations, without claiming flow or initialization safety.
    pub fn resolve_types(&mut self) -> anyhow::Result<SemanticTypes> {
        if self.phase != NamePhase::Resolved {
            return Err(crate::frontend::semantic_type::type_error(
                "resolve_types requires completed names and may only run once",
            ));
        }
        self.phase = NamePhase::Typing;
        let mut types = SemanticTypes::default();
        typing::resolve_project_with_paths(
            &mut self.units,
            &self.symbols,
            &mut types,
            self.paths.clone(),
        )?;
        self.phase = NamePhase::Typed;
        Ok(types)
    }

    pub fn units(&self) -> &[CompilationUnit] {
        &self.units
    }
}

fn add_declaration(
    ns: &mut Namespace,
    name: &str,
    public: bool,
    kind: DeclarationKind,
    span: Span,
    file: bool,
) -> anyhow::Result<DeclId> {
    if file && ns.names.contains_key(name) {
        return Err(at_span(
            name_error(format!("duplicate identifier '{name}'")),
            span,
        ));
    }
    let index = u32::try_from(ns.declarations.len())
        .map_err(|_| name_error("declaration ID space exhausted"))?;
    let id = DeclId {
        file_id: ns.file_id,
        index,
    };
    ns.declarations.push(Declaration {
        name: name.into(),
        visibility: if public {
            Visibility::Public
        } else {
            Visibility::Private
        },
        span,
        kind,
        alias_target: None,
    });
    if file {
        ns.names.insert(name.into(), id);
    }
    Ok(id)
}

fn inline_type(expr: Option<&crate::frontend::ast::expression::Expression>) -> bool {
    matches!(expr.map(|e| &e.kind), Some(ExpressionKind::TypeValue(_)))
}

fn computed_type(expr: Option<&crate::frontend::ast::expression::Expression>) -> bool {
    matches!(expr.map(|e| &e.kind), Some(ExpressionKind::Call { .. }))
}

fn collect_variable(
    ns: &mut Namespace,
    var: &mut VariableDeclaration,
    span: Span,
    inline: bool,
    computed: bool,
) -> anyhow::Result<()> {
    let kind = if var.is_comp {
        if inline {
            DeclarationKind::Type
        } else if computed {
            DeclarationKind::Value
        } else {
            DeclarationKind::Alias
        }
    } else {
        DeclarationKind::Value
    };
    var.annotation.binding = Some(BindingId::Declaration(add_declaration(
        ns,
        &var.identifier,
        var.is_pub,
        kind,
        span,
        true,
    )?));
    Ok(())
}

pub(super) fn at_span(error: anyhow::Error, span: Span) -> anyhow::Error {
    match error.downcast::<CompilerError>() {
        Ok(mut error) => {
            if error.span.is_none() {
                error.span = Some(span);
            }
            error.into()
        }
        Err(error) => error,
    }
}

pub(super) fn with_path(error: anyhow::Error, path: Option<&std::path::PathBuf>) -> anyhow::Error {
    match error.downcast::<CompilerError>() {
        Ok(mut error) => {
            if error.filename.is_none() {
                error.filename = path.cloned();
            }
            error.into()
        }
        Err(error) => error,
    }
}

fn validate_names(ast: &mut crate::frontend::ast::Ast) -> anyhow::Result<()> {
    struct Complete;
    impl walk::Visitor for Complete {
        fn name(&mut self, name: &mut QualifiedIdentifier) -> anyhow::Result<()> {
            if name.annotation.resolved.is_none() {
                return Err(name_error(format!(
                    "pending name '{}'",
                    name.segments.join("::")
                )));
            }
            Ok(())
        }
        fn binding(
            &mut self,
            name: &str,
            slot: &mut BindingAnnotation,
            span: Span,
        ) -> anyhow::Result<()> {
            if slot.binding.is_none() {
                return Err(at_span(
                    name_error(format!("pending binding '{name}'")),
                    span,
                ));
            }
            Ok(())
        }
    }
    walk::statements(&mut Complete, &mut ast.statements, true)
}

pub fn get_namespace(filename: String, symbols: &mut ProjectSymbols) -> anyhow::Result<&Namespace> {
    let file_id = symbols
        .files_by_name
        .get(&filename.into())
        .context("file not found")?;
    symbols
        .namespaces
        .get(file_id)
        .context("namespace not found")
}

pub fn resolve_types(
    units: &mut [CompilationUnit],
    symbols: &ProjectSymbols,
    types: &mut SemanticTypes,
) -> anyhow::Result<()> {
    typing::resolve_project(units, symbols, types)
}

/// Validate the type pass independently from initialization and control flow.
pub fn validate_type_annotations(
    units: &mut [CompilationUnit],
    symbols: &ProjectSymbols,
    types: &SemanticTypes,
) -> anyhow::Result<()> {
    typing::validate_annotations(units, symbols, types)
}

/// Initialization and complete flow validation remain separate from typing.
pub fn validate_analysis(_project: &AnalyzedProject) -> anyhow::Result<()> {
    not_implemented("analysis completion and initialization validation")
}

pub(super) fn not_implemented<T>(operation: &str) -> anyhow::Result<T> {
    anyhow::bail!("{operation} is not implemented")
}

mod aliases;
pub mod ast;
mod constant;
mod evaluation;
pub mod expression;
pub mod function_declaration;
pub mod function_signature;
pub mod generics;
pub mod statement;
mod statement_typing;
pub mod swtich_arm;
pub mod type_expression;
pub mod types;
mod typing;
mod validation;
mod value_typing;
pub mod variable_declaration;
mod walk;

#[cfg(test)]
mod test;
