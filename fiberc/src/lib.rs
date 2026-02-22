pub mod ast;
pub mod cli;
pub mod driver;
pub mod hir;
pub mod lexer;
pub mod lowering;
pub mod parser;
pub mod semantic_analysis;
pub mod token;

use std::path::Path;

/// Library API: compile a project file.
/// Returns Ok(()) on success or Err(message) on failure.
pub fn compile_project(path: &Path, debug: bool) -> Result<(), String> {
    driver::compile_project(path, debug)
}
