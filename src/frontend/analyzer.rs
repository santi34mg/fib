//! Analysis architecture sketch: parsed source units are enriched in place.
//! No pass here executes yet. The only lowering boundary is `AnalyzedProject`
//! to `Ir`; analysis never manufactures a separate typed AST or emits IR.

use std::collections::HashMap;

use anyhow::{Context, bail};

use crate::build_plan::{BuildPlan, FileId};
use crate::driver::CompilationUnit;
use crate::frontend::ast::statement::StatementKind;
use crate::frontend::namespace::{DeclId, Declaration, LocalId, Namespace, Visibility};
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

/// Name annotations are inputs to typing; type/member lookup cannot insert
/// lexical bindings. The shared table will also record parameter/local types.
pub struct TypeContext<'a> {
    pub file: FileId,
    pub symbols: &'a ProjectSymbols,
    pub types: &'a mut SemanticTypes,
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
}

impl Analyzer {
    /// Validate that compilation-unit IDs match exactly the named build-plan files,
    /// then construct ProjectSymbols. Do not discover additional files.
    pub fn new(plan: BuildPlan, units: Vec<CompilationUnit>) -> anyhow::Result<Self> {
        let mut files_by_name = HashMap::new();
        let mut plan_file_iter = plan.files.into_iter();
        for unit in units.iter() {
            let file = plan_file_iter
                // validate the unit exists in the plan
                .find(|source_file| source_file.id == unit.source_file_id)
                .context(format!("unit {:?} not found in plan", unit.source_file_id))?;
            files_by_name.insert(file.name, file.id);
        }
        Ok(Self {
            units,
            symbols: ProjectSymbols {
                files_by_name,
                namespaces: HashMap::new(),
            },
            local_ids: LocalIdAllocator::default(),
        })
    }

    pub fn analyze(mut self) -> anyhow::Result<AnalyzedProject> {
        // Planned order: collect declarations, resolve names/aliases, resolve
        // types/members, validate initialization and annotations, then hand off.
        self.collect_declarations()?;
        self.resolve_names()?;
        not_implemented("project analysis")
    }

    /// Predeclare file bindings for forward references; do not resolve uses or
    /// confuse name visibility with top-level initialization order.
    /// Rejects invalid top-level statements.
    pub fn collect_declarations(&mut self) -> anyhow::Result<()> {
        for unit in self.units.iter() {
            let mut declarations = Vec::new();
            let mut names = HashMap::new();
            let mut idx = 0;
            for statement in unit.ast.statements.iter() {
                match statement.kind {
                    StatementKind::VariableDeclaration(ref var_decl) => {
                        declarations.push(Declaration {
                            name: var_decl.identifier.clone(),
                            visibility: if var_decl.is_pub {
                                Visibility::Public
                            } else {
                                Visibility::Private
                            },
                            span: statement.span,
                            alias_target: None,
                        });
                        names.insert(
                            var_decl.identifier.clone(),
                            DeclId {
                                file_id: unit.source_file_id,
                                index: idx as u32,
                            },
                        );
                        idx += 1;
                    }
                    StatementKind::FunctionDeclaration(ref fun_decl) => {
                        declarations.push(Declaration {
                            name: fun_decl.signature.identifier.clone(),
                            visibility: if fun_decl.is_pub {
                                Visibility::Public
                            } else {
                                Visibility::Private
                            },
                            span: statement.span,
                            alias_target: None,
                        });
                        names.insert(
                            fun_decl.signature.identifier.clone(),
                            DeclId {
                                file_id: unit.source_file_id,
                                index: idx as u32,
                            },
                        );
                        idx += 1;
                    }
                    StatementKind::MultiVariableDeclaration {
                        ref bindings,
                        values: _,
                    } => {
                        for var_decl in bindings {
                            declarations.push(Declaration {
                                name: var_decl.identifier.clone(),
                                visibility: if var_decl.is_pub {
                                    Visibility::Public
                                } else {
                                    Visibility::Private
                                },
                                span: statement.span,
                                alias_target: None,
                            });
                            names.insert(
                                var_decl.identifier.clone(),
                                DeclId {
                                    file_id: unit.source_file_id,
                                    index: idx as u32,
                                },
                            );
                            idx += 1;
                        }
                    }
                    ref stmt => bail!("found unexpected top-level statement: {:?}", stmt),
                }
            }
            let namespace = Namespace {
                file_id: unit.source_file_id,
                declarations,
                names,
            };
            // we create the namespace for this compilation unit
            self.symbols
                .namespaces
                .insert(unit.source_file_id, namespace);
        }
        Ok(())
    }

    pub fn resolve_names(&mut self) -> anyhow::Result<()> {
        // Use a ResolveContext per file; allocate project-unique LocalId/LabelId
        // values and annotate uses. Reject alias cycles and illegal redeclarations.
        for unit in self.units.iter_mut() {
            let mut context = ResolveContext {
                file_id: unit.source_file_id,
                lexical: SymbolTable::new(),
                local_ids: &mut self.local_ids,
                symbols: &self.symbols,
            };
            for statement in unit.ast.statements.iter_mut() {
                match statement.kind {
                    StatementKind::FunctionDeclaration(ref mut function_declaration) => {
                        // the function itself is in the namespace, we avoid
                        // resolving locally it so it can be referenced with
                        // namespace semantics without a problem
                        if let Some(ref mut fbody) = function_declaration.body {
                            context.lexical.enter_scope(ScopeKind::Function);
                            for fstmt in fbody.statements.iter_mut() {
                                Self::resolve_name_statement(fstmt, &mut context)?;
                            }
                            context.lexical.exit_scope()?;
                        }
                    }
                    StatementKind::VariableDeclaration(ref mut variable_declaration) => {
                        // the identifier is already in the namespace
                        // we only resolve the rhs if any
                        if let Some(ref mut expression) = variable_declaration.expression {
                            Self::resolve_name_expression(expression, &mut context)?;
                        }
                    }
                    StatementKind::MultiVariableDeclaration {
                        ref mut bindings,
                        values: _,
                    } => {
                        // the identifiers are already in the namespace
                        // we only resolve the rhs if any
                        for variable_declaration in bindings.iter_mut() {
                            if let Some(ref mut expression) = variable_declaration.expression {
                                Self::resolve_name_expression(expression, &mut context)?;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
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
    _units: &mut [CompilationUnit],
    _symbols: &ProjectSymbols,
    _types: &mut SemanticTypes,
) -> anyhow::Result<()> {
    // TypeContext annotates the same syntax, including deferred member lookup.
    not_implemented("project type resolution")
}

/// A successful handoff must not contain pending annotations or illegal reads
/// of later, uninitialized file variables. This is distinct from name lookup.
pub fn validate_analysis(_project: &AnalyzedProject) -> anyhow::Result<()> {
    not_implemented("analysis completion and initialization validation")
}

pub(super) fn not_implemented<T>(operation: &str) -> anyhow::Result<T> {
    anyhow::bail!("{operation} is not implemented")
}

pub mod ast;
pub mod expression;
pub mod function_declaration;
pub mod function_signature;
pub mod generics;
pub mod statement;
pub mod swtich_arm;
pub mod type_expression;
pub mod types;
pub mod variable_declaration;

#[cfg(test)]
mod test;
