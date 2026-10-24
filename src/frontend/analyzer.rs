//! Staged name/type analysis enriches parsed source units in place.
//! Names, expressions and members resolve; complete analysis checks definite
//! initialization and structured control flow before handing off the source AST.
//! The only lowering boundary is `AnalyzedProject` to `Ir`; analysis never
//! manufactures a separate typed AST or emits IR.

use std::collections::{HashMap, HashSet};

use anyhow::Context;

use crate::builtins::{BuiltinFunction, SyntheticModule};
use crate::diagnostics::{CompilerError, Span};
use crate::driver::CompilationUnit;
use crate::frontend::analysis_annotation::{
    BindingAnnotation, BindingId, ExpressionInfo, ValueCategory,
};
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
    namespace::ProjectSymbols,
    semantic_type::{SemanticType, SemanticTypes},
    symbol_table::SymbolTable,
};
use crate::plan::{BuildPlan, FileId, FileName};

/// The original ASTs plus project-owned semantic identities and types.
/// AST nodes carry initially pending `analysis_annotation` slots. Successful
/// analysis must leave every applicable slot resolved; public fields alone
/// do not certify that the completion-validation pass has run.
#[derive(Debug, Clone)]
pub struct AnalyzedProject {
    pub entry: FileId,
    pub warnings: Vec<crate::diagnostics::CompilerWarning>,
    pub units: Vec<CompilationUnit>,
    pub symbols: ProjectSymbols,
    pub types: SemanticTypes,
    /// Build-environment functions keyed by their synthetic declaration identity.
    pub builtins: HashMap<DeclId, BuiltinFunction>,
    /// Original plan paths retained for diagnostics after analysis.
    pub source_paths: HashMap<FileId, std::path::PathBuf>,
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
    entry: FileId,
    units: Vec<CompilationUnit>,
    symbols: ProjectSymbols,
    local_ids: LocalIdAllocator,
    paths: HashMap<FileId, std::path::PathBuf>,
    synthetic_modules: Vec<(FileId, SyntheticModule)>,
    builtins: HashMap<DeclId, BuiltinFunction>,
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
        Self {
            entry: plan.entry,
            units,
            symbols,
            local_ids: LocalIdAllocator::default(),
            paths: plan.files.into_iter().map(|f| (f.id, f.path)).collect(),
            synthetic_modules: Vec::new(),
            builtins: HashMap::new(),
            phase: NamePhase::New,
        }
        .with_synthetic_module(SyntheticModule::MEMORY)
    }

    /// Add a compiler-provided namespace to the same analysis pipeline used by
    /// source modules. Synthetic declarations have identities and semantic types,
    /// but no source AST or path.
    pub fn with_synthetic_module(mut self, module: SyntheticModule) -> anyhow::Result<Self> {
        if self
            .symbols
            .files_by_name
            .contains_key(&FileName(module.name.into()))
        {
            return Err(name_error(format!(
                "duplicate file or synthetic module name '{}'",
                module.name
            )));
        }
        let next = self
            .symbols
            .files_by_name
            .values()
            .chain(self.synthetic_modules.iter().map(|(id, _)| id))
            .map(|id| id.0)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| name_error("synthetic module file ID space exhausted"))?;
        let file = FileId(next);
        self.symbols
            .files_by_name
            .insert(FileName(module.name.into()), file);
        self.synthetic_modules.push((file, module));
        Ok(self)
    }

    pub fn analyze(mut self) -> anyhow::Result<AnalyzedProject> {
        // Planned order: collect declarations, resolve names/aliases, resolve
        // types/members, validate initialization and annotations, then hand off.
        self.collect_declarations()?;
        self.resolve_names()?;
        let types = self.resolve_types()?;
        let mut project = AnalyzedProject {
            entry: self.entry,
            warnings: Vec::new(),
            units: self.units,
            symbols: self.symbols,
            types,
            builtins: self.builtins,
            source_paths: self.paths.clone(),
        };
        validate_analysis_with_paths(&project, Some(&self.paths))?;
        struct Warnings(
            Vec<crate::diagnostics::CompilerWarning>,
            Option<std::path::PathBuf>,
        );
        impl walk::Visitor for Warnings {
            fn expression(
                &mut self,
                node: &mut crate::frontend::ast::expression::Expression,
            ) -> anyhow::Result<()> {
                if matches!(&node.kind, crate::frontend::ast::expression::ExpressionKind::ExprAcess { member, .. } if member.implicit_deref)
                {
                    let mut diagnostic = CompilerError::new(
                        crate::diagnostics::ErrorKind::Type,
                        "implicit pointer dereference in field access".into(),
                    )
                    .with_span(node.span)
                    .with_hint("use an explicit postfix dereference to make the access explicit");
                    diagnostic.filename = self.1.clone();
                    self.0
                        .push(crate::diagnostics::CompilerWarning { diagnostic });
                }
                Ok(())
            }
        }
        for unit in &mut project.units {
            let mut warnings = Warnings(Vec::new(), self.paths.get(&unit.source_file_id).cloned());
            walk::statements(&mut warnings, &mut unit.ast.statements, true)?;
            project.warnings.extend(warnings.0);
        }
        let mut counts = HashMap::new();
        for warning in &project.warnings {
            *counts
                .entry((warning.diagnostic.filename.clone(), warning.diagnostic.span))
                .or_insert(0usize) += 1;
        }
        for instance in project.types.generic_instances.values_mut() {
            if let Some(function) = &mut instance.body
                && let Some(body) = &mut function.body
            {
                let mut warnings = Warnings(
                    Vec::new(),
                    self.paths.get(&instance.key.template.file_id).cloned(),
                );
                walk::statements(&mut warnings, &mut body.statements, false)?;
                let mut instance_counts = HashMap::new();
                for warning in warnings.0 {
                    let key = (warning.diagnostic.filename.clone(), warning.diagnostic.span);
                    let current = instance_counts.entry(key.clone()).or_insert(0usize);
                    *current += 1;
                    let count = counts.entry(key).or_insert(0usize);
                    if *current > *count {
                        *count = *current;
                        project.warnings.push(warning);
                    }
                }
            }
        }
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
                            if let Some(scope) = fun.pub_scope {
                                namespace
                                    .declarations
                                    .last_mut()
                                    .expect("function declaration")
                                    .visibility = scope;
                            }
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
        for &(file_id, module) in &self.synthetic_modules {
            let mut namespace = Namespace {
                file_id,
                declarations: Vec::new(),
                names: HashMap::new(),
            };
            for &builtin in module.functions {
                let id = add_declaration(
                    &mut namespace,
                    builtin.name(),
                    true,
                    DeclarationKind::Value,
                    Span::new(1, 1),
                    true,
                )?;
                self.builtins.insert(id, builtin);
            }
            self.symbols.namespaces.insert(file_id, namespace);
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
        // also check builtin types
        for (&id, &builtin) in &self.builtins {
            let signature = builtin.semantic_type(&mut types);
            let ty = types.intern(signature);
            types.bind(
                BindingId::Declaration(id),
                ExpressionInfo {
                    ty,
                    category: ValueCategory::Value,
                    type_value: None,
                },
            );
        }
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
    if crate::frontend::tokens::builtin::BuiltinType::from_name(name).is_some() {
        return Err(at_span(
            name_error(format!("prelude type name '{name}' cannot be shadowed")),
            span,
        ));
    }
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
    matches!(
        expr.map(|e| &e.kind),
        Some(
            ExpressionKind::Call { .. }
                | ExpressionKind::IndexAccess { .. }
                | ExpressionKind::SliceLiteral { .. }
        )
    )
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
    if let Some(scope) = var.pub_scope {
        ns.declarations
            .last_mut()
            .expect("variable declaration")
            .visibility = scope;
    }
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

/// The same statically known condition facts used by structured flow checking.
pub(crate) fn known_boolean(
    expression: &crate::frontend::ast::expression::Expression,
) -> Option<bool> {
    evaluation::boolean(expression, &HashMap::new())
}

/// Independently validate annotations, definite initialization and structured flow.
pub fn validate_analysis(project: &AnalyzedProject) -> anyhow::Result<()> {
    validate_analysis_with_paths(project, Some(&project.source_paths))
}

fn validate_analysis_with_paths(
    project: &AnalyzedProject,
    paths: Option<&HashMap<FileId, std::path::PathBuf>>,
) -> anyhow::Result<()> {
    if !project.types.complete {
        return Err(crate::frontend::semantic_type::type_error(
            "analysis requires completed type resolution",
        ));
    }
    for (&id, &builtin) in &project.builtins {
        if builtin.is_intrinsic() {
            let memory = project
                .symbols
                .files_by_name
                .get(&FileName("memory".into()));
            if memory != Some(&id.file_id)
                || project.symbols.declaration(id)?.name != "unchecked_deref"
                || !matches!(
                    project
                        .types
                        .declarations
                        .get(&id)
                        .and_then(|ty| project.types.get(*ty).ok()),
                    Some(SemanticType::IntrinsicFunction)
                )
            {
                return Err(crate::frontend::semantic_type::type_error(
                    "invalid memory intrinsic identity",
                ));
            }
        }
    }
    let mut units = project.units.clone();
    validate_type_annotations(&mut units, &project.symbols, &project.types)?;
    flow::validate(project, paths)
}

mod aliases;
pub mod ast;
mod constant;
mod evaluation;
pub mod expression;
mod flow;
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

pub(crate) use constant::{evaluate_boolean, evaluate_numeric};
