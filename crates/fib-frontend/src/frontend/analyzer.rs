//! Staged name/type analysis enriches parsed source units in place.
//! Names, expressions and members resolve; complete analysis checks definite
//! initialization and structured control flow before handing off the source AST.
//! The only lowering boundary is `AnalyzedProject` to `Ir`; analysis never
//! manufactures a separate typed AST or emits IR.

use std::collections::{HashMap, HashSet};

use anyhow::Context;

use crate::CompilationUnit;
use crate::builtins::{BuiltinFunction, SyntheticModule};
use crate::diagnostics::{CompilerError, Span};
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
    /// Compiler-owned operations keyed by their canonical declaration identity.
    pub builtins: HashMap<DeclId, BuiltinFunction>,
    /// Original plan paths retained for diagnostics after analysis.
    pub source_paths: HashMap<FileId, std::path::PathBuf>,
}

/// Scope snapshots at name sites, retained only by collecting analysis.
pub(crate) type LexicalCompletions = HashMap<
    (FileId, Span),
    std::collections::BTreeMap<String, crate::frontend::identifier::ResolvedName>,
>;

/// Partial semantic information for tooling; not a lowering input.
#[derive(Debug)]
pub struct AnalysisReport {
    pub(crate) lexical_completions: LexicalCompletions,
    /// Files whose syntax or source was unavailable; their namespaces are blocked.
    pub unavailable_files: HashSet<FileId>,
    pub errors: Vec<CompilerError>,
    pub warnings: Vec<crate::diagnostics::CompilerWarning>,
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
    pub recovery: Option<Recovery>,
}

impl ResolveContext<'_> {
    fn canonical_entity(
        &self,
        entity: crate::frontend::identifier::Entity,
    ) -> anyhow::Result<crate::frontend::identifier::Entity> {
        use crate::frontend::identifier::Entity;
        let blocked = |entity| {
            self.recovery.as_ref().is_some_and(|r| match entity {
                Entity::File(file) => r.unavailable(file),
                Entity::Declaration(id) => r.blocked(BindingId::Declaration(id)),
            })
        };
        if blocked(entity) {
            return Err(recovery::Blocked.into());
        }
        let canonical = self.symbols.canonical(entity)?;
        if blocked(canonical) {
            return Err(recovery::Blocked.into());
        }
        Ok(canonical)
    }

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
    recovery: Option<Recovery>,
}

impl<'a> TypeContext<'a> {
    pub fn new(file: FileId, symbols: &'a ProjectSymbols, types: &'a mut SemanticTypes) -> Self {
        Self {
            file,
            symbols,
            types,
            state: typing::TypeState::default(),
            recovery: None,
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
    recovery: Option<Recovery>,
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
        let registration = plan.standard_library.loaded.clone();
        let mut analyzer = Self {
            entry: plan.entry,
            units,
            symbols,
            local_ids: LocalIdAllocator::default(),
            paths: plan.files.into_iter().map(|f| (f.id, f.path)).collect(),
            synthetic_modules: Vec::new(),
            builtins: HashMap::new(),
            phase: NamePhase::New,
            recovery: None,
        };
        if let Some(std) = registration {
            for file in [std.memory, std.os, std.private, std.prelude] {
                anyhow::ensure!(
                    analyzer
                        .symbols
                        .regions
                        .get(&file)
                        .is_some_and(|r| r.package == std.package),
                    "invalid std module registration"
                );
            }
            for (file, module) in [
                (std.memory, SyntheticModule::MEMORY),
                (std.os, SyntheticModule::OS),
                (std.private, SyntheticModule::OS_PRIVATE),
            ] {
                analyzer = analyzer.with_module_intrinsics(file, module)?;
            }
        }
        analyzer.with_synthetic_module(SyntheticModule::CORE)
    }

    /// Add a compiler-provided namespace to the same analysis pipeline used by
    /// source modules. Synthetic declarations have identities and semantic types,
    /// but no source AST or path.
    pub fn with_synthetic_module(mut self, module: SyntheticModule) -> anyhow::Result<Self> {
        let name = crate::frontend::namespace::normalize_path(module.name)?;
        if self
            .symbols
            .files_by_name
            .contains_key(&FileName(name.clone()))
            || self
                .symbols
                .package_files
                .keys()
                .any(|(_, path)| path == &name)
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
            .chain(self.symbols.regions.keys())
            .chain(self.synthetic_modules.iter().map(|(id, _)| id))
            .map(|id| id.0)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .filter(|id| *id != u32::MAX)
            .ok_or_else(|| name_error("synthetic module file ID space exhausted"))?;
        let file = FileId(next);
        self.symbols.files_by_name.insert(FileName(name), file);
        self.with_module_intrinsics(file, module)
    }

    /// Augment a registered source or synthetic module with compiler declarations.
    /// Both origins share declaration collection, visibility and alias resolution.
    pub fn with_module_intrinsics(
        mut self,
        file: FileId,
        module: SyntheticModule,
    ) -> anyhow::Result<Self> {
        anyhow::ensure!(
            self.phase == NamePhase::New,
            "compiler modules must be registered before collection"
        );
        anyhow::ensure!(
            self.symbols.regions.contains_key(&file)
                || self.symbols.files_by_name.values().any(|id| *id == file),
            "unknown compiler module file identity"
        );
        anyhow::ensure!(
            !self.synthetic_modules.iter().any(|(id, _)| *id == file),
            "duplicate compiler module registration"
        );
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
        collect_warnings(
            &mut project.units,
            &mut project.types,
            &self.paths,
            &mut project.warnings,
        )?;
        Ok(project)
    }

    /// Analyze independent regions for tooling. Failed annotations remain pending;
    /// this report is never accepted by lowering.
    pub fn analyze_collecting(self) -> AnalysisReport {
        self.collect_available(HashSet::new())
    }

    /// Parsed files may be unavailable after a syntax or IO error. Their file
    /// identities remain known, but dependent checks cannot succeed.
    pub fn analyze_available(
        plan: BuildPlan,
        mut units: Vec<CompilationUnit>,
        unavailable: HashSet<FileId>,
        build_file: bool,
    ) -> anyhow::Result<AnalysisReport> {
        for &file in &unavailable {
            anyhow::ensure!(
                plan.files.iter().any(|f| f.id == file)
                    && !units.iter().any(|u| u.source_file_id == file),
                "invalid unavailable source identity"
            );
            units.push(CompilationUnit {
                source_file_id: file,
                ast: crate::frontend::ast::Ast::from(Vec::new()),
            });
        }
        let mut analyzer = Self::new(plan, units)?;
        if build_file {
            analyzer = analyzer.with_synthetic_module(SyntheticModule::BUILD_API)?;
        }
        Ok(analyzer.collect_available(unavailable))
    }

    fn collect_available(mut self, unavailable: HashSet<FileId>) -> AnalysisReport {
        let recovery = Recovery::with_paths(self.paths.clone());
        recovery.set_unavailable(unavailable.clone());
        self.recovery = Some(recovery.clone());
        let mut types = SemanticTypes::default();
        let result = (|| {
            self.collect_declarations()?;
            self.resolve_names()?;
            types = self.resolve_types()?;
            flow::validate_collecting(&self.units, &types, &self.paths, &recovery)?;
            Ok(())
        })();
        if let Err(error) = result {
            recovery.record(
                error,
                self.entry,
                Span::new(1, 1),
                self.paths.get(&self.entry),
            );
        }
        let mut warnings = Vec::new();
        if let Err(error) =
            collect_warnings(&mut self.units, &mut types, &self.paths, &mut warnings)
        {
            recovery.record(
                error,
                self.entry,
                Span::new(1, 1),
                self.paths.get(&self.entry),
            );
        }
        let mut errors = recovery.diagnostics();
        errors.sort_by_key(|d| {
            (
                d.filename.clone(),
                d.span.map(|s| (s.line, s.column)),
                d.message.clone(),
            )
        });
        self.units
            .retain(|u| !unavailable.contains(&u.source_file_id));
        AnalysisReport {
            lexical_completions: recovery.lexical(),
            unavailable_files: unavailable,
            errors,
            warnings,
            units: self.units,
            symbols: self.symbols,
            types,
        }
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
                    let previous_statement = self.recovery.as_ref().map(|_| statement.clone());
                    let previous_namespace = self.recovery.as_ref().map(|_| namespace.clone());
                    let result = (|| {
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
                        Ok(())
                    })();
                    if let Err(error) = result {
                        if let Some(recovery) = &self.recovery {
                            namespace = previous_namespace.unwrap();
                            *statement = previous_statement.unwrap();
                            recovery.record(
                                at_span(error, span),
                                file_id,
                                span,
                                self.paths.get(&file_id),
                            );
                        } else {
                            return Err(error);
                        }
                    }
                }
                // Reserve identities without adding lexical names or resolving initializers.
                struct LocalTypes<'a>(&'a mut Namespace, Option<Recovery>, FileId);
                impl walk::Visitor for LocalTypes<'_> {
                    fn variable(
                        &mut self,
                        var: &mut VariableDeclaration,
                        span: Span,
                        file: bool,
                        inline: bool,
                    ) -> anyhow::Result<()> {
                        if !file && var.is_comp && inline {
                            match add_declaration(
                                self.0,
                                &var.identifier,
                                false,
                                DeclarationKind::Type,
                                span,
                                false,
                            ) {
                                Ok(id) => var.annotation.binding = Some(BindingId::Declaration(id)),
                                Err(error) => {
                                    if let Some(recovery) = &self.1 {
                                        recovery.record(error, self.2, span, None);
                                    } else {
                                        return Err(error);
                                    }
                                }
                            }
                        }
                        Ok(())
                    }
                }
                walk::statements(
                    &mut LocalTypes(&mut namespace, self.recovery.clone(), file_id),
                    &mut unit.ast.statements,
                    true,
                )?;
                self.symbols.namespaces.insert(file_id, namespace);
                Ok(())
            })();
            result.map_err(|e| with_path(e, self.paths.get(&file_id)))?;
        }
        for &(file_id, module) in &self.synthetic_modules {
            let mut namespace =
                self.symbols
                    .namespaces
                    .remove(&file_id)
                    .unwrap_or_else(|| Namespace {
                        file_id,
                        declarations: Vec::new(),
                        names: HashMap::new(),
                    });
            for &builtin in module.functions {
                let id = add_declaration(
                    &mut namespace,
                    builtin.name(),
                    true,
                    DeclarationKind::Value,
                    Span::new(1, 1),
                    true,
                )?;
                if matches!(
                    builtin,
                    BuiltinFunction::PointerFromWord | BuiltinFunction::PointerToWord
                ) {
                    namespace.declarations[id.index as usize].visibility = Visibility::Package;
                }
                self.builtins.insert(id, builtin);
                self.symbols.compiler_declarations.insert(id, builtin);
            }
            self.symbols.namespaces.insert(file_id, namespace);
        }
        for (&file, ns) in &self.symbols.namespaces {
            for name in ns.names.keys() {
                if self.symbols.reserved_package_root(file, name) {
                    return Err(name_error(format!(
                        "package name '{name}' cannot be shadowed"
                    )));
                }
            }
        }
        self.install_library_prelude()?;
        self.phase = NamePhase::Collected;
        Ok(())
    }

    fn install_library_prelude(&mut self) -> anyhow::Result<()> {
        let Some(file) = self.symbols.library_prelude else {
            return Ok(());
        };
        let package = self.symbols.regions[&file].package;
        let mut exports = self.symbols.namespaces[&file]
            .names
            .iter()
            .filter(|(_, id)| {
                self.symbols
                    .declaration(**id)
                    .is_ok_and(|d| d.visibility == Visibility::Public)
            })
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>();
        exports.sort();
        if exports.is_empty() {
            return Ok(());
        }
        let source = exports
            .iter()
            .map(|name| format!("comp {name} := std::prelude::{name};"))
            .collect::<Vec<_>>()
            .join("\n");
        for unit in &mut self.units {
            if self.symbols.regions[&unit.source_file_id].package == package {
                continue;
            }
            for name in &exports {
                if self
                    .symbols
                    .reserved_package_root(unit.source_file_id, name)
                {
                    return Err(name_error(format!(
                        "package name '{name}' cannot be shadowed by a prelude export"
                    )));
                }
            }
            let path = &self.paths[&unit.source_file_id];
            let mut ast = crate::frontend::parser::Parser::with_source(
                crate::frontend::tokenizer::Tokenizer::new(&source),
                path,
                &source,
            )
            .parse()?;
            let ns = self
                .symbols
                .namespaces
                .get_mut(&unit.source_file_id)
                .unwrap();
            for node in &mut ast.statements {
                let StatementKind::VariableDeclaration(var) = &mut node.kind else {
                    unreachable!()
                };
                // These aliases have no textual occurrence in the consuming
                // document. Do not let tooling classify unrelated user tokens
                // at positions from the generated declaration text.
                var.annotation.source_span = None;
                if let Some(crate::frontend::ast::expression::Expression {
                    kind: ExpressionKind::QualifiedIdentifier(name),
                    ..
                }) = &mut var.expression
                {
                    name.annotation.source_span = None;
                }
                collect_variable(ns, var, node.span, false, false)?;
            }
            unit.ast.statements.extend(ast.statements);
        }
        Ok(())
    }

    pub fn resolve_names(&mut self) -> anyhow::Result<()> {
        if self.phase != NamePhase::Collected {
            return Err(name_error(
                "resolve_names requires declaration collection and may only run once",
            ));
        }
        self.phase = NamePhase::Resolving;
        let aliases = aliases::definitions(&self.units, &self.paths, self.recovery.as_ref())?;
        aliases::resolve(
            &mut self.symbols,
            &aliases,
            &self.paths,
            self.recovery.as_ref(),
        )?;
        for unit in &mut self.units {
            let file = unit.source_file_id;
            let mut context = ResolveContext {
                file_id: file,
                lexical: SymbolTable::new(),
                local_ids: &mut self.local_ids,
                recovery: self.recovery.clone(),
                symbols: &self.symbols,
            };
            unit.ast
                .resolve_name(&mut context)
                .map_err(|e| with_path(e, self.paths.get(&file)))?;
            if self.recovery.is_none() {
                validate_names(&mut unit.ast).map_err(|e| with_path(e, self.paths.get(&file)))?;
            }
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
            let signature = crate::builtins::semantic_type(builtin, &mut types);
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
        typing::resolve_project_collecting(
            &mut self.units,
            &self.symbols,
            &mut types,
            self.paths.clone(),
            self.recovery.clone(),
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
    expr.is_some_and(|e| {
        !matches!(
            e.kind,
            ExpressionKind::QualifiedIdentifier(_) | ExpressionKind::TypeValue(_)
        )
    })
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
pub fn known_boolean(expression: &crate::frontend::ast::expression::Expression) -> Option<bool> {
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
    if project.builtins != project.symbols.compiler_declarations {
        return Err(crate::frontend::semantic_type::type_error(
            "compiler declaration registry mismatch",
        ));
    }
    for (&id, &builtin) in &project.builtins {
        project.symbols.declaration(id)?;
        if builtin.is_intrinsic()
            && !matches!(
                project
                    .types
                    .declarations
                    .get(&id)
                    .and_then(|ty| project.types.get(*ty).ok()),
                Some(SemanticType::IntrinsicFunction)
            )
        {
            return Err(crate::frontend::semantic_type::type_error(
                "invalid intrinsic identity",
            ));
        }
    }
    let mut units = project.units.clone();
    validate_type_annotations(&mut units, &project.symbols, &project.types)?;
    flow::validate(project, paths)
}

mod aliases;
mod recovery;
pub use recovery::Recovery;
pub mod ast;
mod constant;
pub mod constant_data;
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
pub(crate) mod walk;

#[cfg(test)]
mod test;

pub use constant::{evaluate_boolean, evaluate_numeric};

fn collect_warnings(
    units: &mut [CompilationUnit],
    types: &mut SemanticTypes,
    paths: &HashMap<FileId, std::path::PathBuf>,
    warnings: &mut Vec<crate::diagnostics::CompilerWarning>,
) -> anyhow::Result<()> {
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
    for unit in units {
        let mut collector = Warnings(Vec::new(), paths.get(&unit.source_file_id).cloned());
        walk::statements(&mut collector, &mut unit.ast.statements, true)?;
        warnings.extend(collector.0);
    }
    let mut counts = HashMap::new();
    for warning in warnings.iter() {
        *counts
            .entry((warning.diagnostic.filename.clone(), warning.diagnostic.span))
            .or_insert(0usize) += 1;
    }
    for instance in types.generic_instances.values_mut() {
        if let Some(function) = &mut instance.body
            && let Some(body) = &mut function.body
        {
            let mut collector = Warnings(
                Vec::new(),
                paths.get(&instance.key.template.file_id).cloned(),
            );
            walk::statements(&mut collector, &mut body.statements, false)?;
            let mut instance_counts = HashMap::new();
            for warning in collector.0 {
                let key = (warning.diagnostic.filename.clone(), warning.diagnostic.span);
                let current = instance_counts.entry(key.clone()).or_insert(0usize);
                *current += 1;
                let count = counts.entry(key).or_insert(0usize);
                if *current > *count {
                    *count = *current;
                    warnings.push(warning);
                }
            }
        }
    }
    Ok(())
}
