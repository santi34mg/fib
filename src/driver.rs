use anyhow::bail;
use std::path::Path;

use crate::builder_object::BuildStage;
#[cfg(feature = "llvm")]
use crate::builder_object::BuilderObject;
use crate::driver::compiler_output::CompilationOutput;
use crate::frontend::analyzer::Analyzer;
use crate::frontend::parser::Parser;
use crate::frontend::tokenizer::Tokenizer;
use crate::ir::Ir;

/// `run_pipeline` is the main connector between different parts of the compiler.
pub fn run_pipeline(path: &Path, bo: &BuilderObject) -> anyhow::Result<CompilationOutput> {
    match bo.build_stage {
        BuildStage::BuildFileCompilation => Ok(CompilationOutput::Ir(run_frontend(path, bo)?)),
        BuildStage::IndividualIrCompilation => Ok(CompilationOutput::ObjectFile(todo!())),
        BuildStage::Linking => Ok(CompilationOutput::Binary(todo!())),
        _ => bail!("build stage unsupported"),
    }
}

/// Run lex + parse + import resolution + analysis. No LLVM involved, so this
/// works with `--no-default-features` and is unit-testable without clang.
pub fn run_frontend(
    path: &Path,
    // TODO: consider removing builer object from here
    bo: &BuilderObject,
) -> anyhow::Result<Ir> {
    let _ = bo;
    let mut source = std::fs::read_to_string(path)?;

    let tokens = Tokenizer::new(&mut source);

    let mut parser = Parser::new(tokens, path);
    let ast = parser.parse()?;

    let analyzer = Analyzer::new(ast);
    analyzer.analyze()
}

pub mod compiler_output;
mod test;
