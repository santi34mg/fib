use std::path::PathBuf;

use crate::diagnostics::CompilerError;
use crate::driver::{self, CompilationOptions, CompileOutput};

/// Configures a compilation without selecting the unfinished build-file pipeline.
///
/// For now this is an opt-in entry point over the existing driver. The CLI and
/// library APIs continue to use `CompilationOptions` directly.
#[derive(Debug, Clone)]
pub struct BuilderObject {
    options: CompilationOptions,
}

impl BuilderObject {
    pub fn new(project_path: impl Into<PathBuf>) -> Self {
        Self {
            options: CompilationOptions::new(project_path.into()),
        }
    }

    /// Configure the same options accepted by the current compiler driver.
    pub fn options_mut(&mut self) -> &mut CompilationOptions {
        &mut self.options
    }

    pub fn build(&self) -> Result<CompileOutput, Box<CompilerError>> {
        driver::compile(&self.options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::EmitKind;

    #[test]
    fn builds_with_existing_frontend_pipeline() {
        let mut builder = BuilderObject::new("test.fib");
        builder.options_mut().source_override = Some("fn main() @int { return 0; }".into());
        builder.options_mut().check_only = true;

        let CompileOutput::Frontend { emit, frontend } = builder.build().expect("compile") else {
            panic!("expected frontend output");
        };
        assert_eq!(emit, EmitKind::Typed);
        assert_eq!(frontend.typed_program.declarations.len(), 1);
    }

    #[test]
    fn preserves_driver_diagnostics() {
        let mut builder = BuilderObject::new("test.fib");
        builder.options_mut().source_override = Some("fn main() @int { return missing; }".into());
        builder.options_mut().check_only = true;

        let error = builder.build().expect_err("expected analysis error");
        assert!(error.to_string().contains("missing"));
    }
}
