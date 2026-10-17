//! Analysis architecture sketch: parsed source units are enriched in place.
//! No pass here executes yet. The only lowering boundary is `AnalyzedProject`
//! to `Ir`; analysis never manufactures a separate typed AST or emits IR.

use crate::build_plan::{BuildPlan, FileId};
use crate::driver::SourceUnit;
use crate::frontend::{
    namespace::ProjectSymbols, semantic_type::SemanticTypes, symbol_table::SymbolTable,
};

/// The original ASTs plus project-owned semantic identities and types.
/// AST nodes carry initially pending `analysis_annotation` slots. Successful
/// analysis must leave every applicable slot resolved; public fields alone
/// do not certify that the completion-validation pass has run.
#[derive(Debug, Clone)]
pub struct AnalyzedProject {
    pub units: Vec<SourceUnit>,
    pub symbols: ProjectSymbols,
    pub types: SemanticTypes,
}

/// Name-pass state for one file. File namespaces are shared, never copied into
/// lexical scopes. Locals take precedence over unqualified file declarations;
/// qualified file selection bypasses that local binding. Labels are separate.
pub struct ResolveContext<'a> {
    pub file: FileId,
    pub symbols: &'a mut ProjectSymbols,
    pub lexical: SymbolTable,
}

/// Name annotations are inputs to typing; type/member lookup cannot insert
/// lexical bindings. The shared table will also record parameter/local types.
pub struct TypeContext<'a> {
    pub file: FileId,
    pub symbols: &'a ProjectSymbols,
    pub types: &'a mut SemanticTypes,
}

pub struct Analyzer {
    units: Vec<SourceUnit>,
    symbols: ProjectSymbols,
}

impl Analyzer {
    pub fn new(_plan: &BuildPlan, _units: Vec<SourceUnit>) -> anyhow::Result<Self> {
        // Validate that source-unit IDs match exactly the named build-plan files,
        // then construct ProjectSymbols. Do not discover additional files.
        not_implemented("analyzer construction")
    }

    pub fn analyze(self) -> anyhow::Result<AnalyzedProject> {
        // Planned order: collect declarations, resolve names/aliases, resolve
        // types/members, validate initialization and annotations, then hand off.
        let _inputs = (self.units, self.symbols);
        not_implemented("project analysis")
    }
}

pub fn collect_declarations(
    _units: &[SourceUnit],
    _symbols: &mut ProjectSymbols,
) -> anyhow::Result<()> {
    // Predeclare file bindings for forward references; do not resolve uses or
    // confuse name visibility with top-level initialization order.
    not_implemented("declaration collection")
}

pub fn resolve_names(
    _units: &mut [SourceUnit],
    _symbols: &mut ProjectSymbols,
) -> anyhow::Result<()> {
    // Use a ResolveContext per file; allocate project-unique LocalId/LabelId
    // values and annotate uses. Reject alias cycles and illegal redeclarations.
    not_implemented("project name resolution")
}

pub fn resolve_types(
    _units: &mut [SourceUnit],
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
pub mod type_declaration;
pub mod type_expression;
pub mod types;
pub mod variable_declaration;

#[cfg(test)]
mod test;
