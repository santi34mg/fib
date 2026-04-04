pub mod analysis;
pub mod ast;
pub mod cli;
pub mod driver;
pub mod hir;
pub mod lexing;
#[cfg(feature = "llvm")]
pub mod lowering;
pub mod parsing;
pub mod tokens;

#[cfg(feature = "llvm")]
use std::error::Error;

#[cfg(feature = "llvm")]
use crate::driver::CompilationOptions;

/// Library API: compile a project file.
/// `include_paths` is a list of additional directories searched when resolving imports.
/// Returns Ok(()) on success or Err(message) on failure.
#[cfg(feature = "llvm")]
pub fn compile_project(compilation_options: CompilationOptions) -> Result<(), Box<dyn Error>> {
    driver::compile(compilation_options)
}
