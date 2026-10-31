//! Parsing, semantic analysis, source planning, and editor services.
pub mod builtins;
pub mod diagnostics;
pub mod frontend;
pub use frontend::*;
pub mod numeric;
pub mod plan;
pub mod standard_library;

use frontend::ast::Ast;
use plan::FileId;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CompilationUnit {
    /// ID of the source file that generated this unit
    pub source_file_id: FileId,
    pub ast: Ast,
}
