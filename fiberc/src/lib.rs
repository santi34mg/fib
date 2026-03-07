pub mod ast;
pub mod cli;
pub mod driver;
pub mod hir;
pub mod lexer;
pub mod lowering;
pub mod parser;
pub mod analysis;
pub mod token;

use std::path::Path;

const DEBUG_TARGET: &str = "debug";

/// Library API: compile a project file.
/// Returns Ok(()) on success or Err(message) on failure.
pub fn compile_project(path: &Path, target: String) -> Result<(), Box<dyn std::error::Error>> {
    let debug = if target == DEBUG_TARGET { true } else { false };
    driver::compile(path, debug)
}
