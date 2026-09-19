pub mod backend;
pub mod cli;
pub mod diagnostics;
pub mod driver;
pub mod frontend;
pub mod ir;

pub use diagnostics::CompilerError;
pub use driver::{CompilationOptions, CompileOutput, DriverError, EmitKind};

/// Library API: compile a project file.
///
/// Frontend-only emits (`lex|parse|typed`, `check_only`) work without the
/// `llvm` cargo feature; backend emits (`llvm|bin`) return
/// `CompilerError::Toolchain` when LLVM is disabled.
/// Returns what was produced (no stdout parsing needed).
pub fn compile_project(
    compilation_options: &CompilationOptions,
) -> Result<CompileOutput, Box<CompilerError>> {
    driver::compile(compilation_options)
}

/// Library API: run only the frontend (lex + parse + analyze).
/// Always available, even without the `llvm` feature.
pub fn check_project(
    compilation_options: &CompilationOptions,
) -> Result<driver::FrontendResponse, DriverError> {
    driver::check_project(compilation_options)
}
