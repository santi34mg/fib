use anyhow::Context;

use crate::build_plan::{BuildPlan, FileId};
use crate::driver::compiler_output::CompilationOutput;
use crate::frontend::analyzer::AnalyzedProject;
use crate::frontend::ast::Ast;
use crate::ir::Ir;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineGoal {
    Check,
    Ir,
    Objects,
    Binary,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CompilationUnit {
    /// ID of the source file that generated this unit
    pub source_file_id: FileId,
    pub ast: Ast,
}

/// Compile only the files explicitly enumerated and named by the plan.
pub fn run_pipeline(_plan: &BuildPlan, _goal: PipelineGoal) -> anyhow::Result<CompilationOutput> {
    // Run the frontend, then stop at the requested stage or emit and link.
    anyhow::bail!("pipeline orchestration is not implemented")
}

pub fn run_frontend(plan: &BuildPlan) -> anyhow::Result<AnalyzedProject> {
    crate::frontend::analyzer::Analyzer::new(plan.clone(), parse_files(plan)?)?.analyze()
}

pub fn parse_files(plan: &BuildPlan) -> anyhow::Result<Vec<CompilationUnit>> {
    anyhow::ensure!(
        plan.files.iter().any(|file| file.id == plan.entry),
        "build-plan entry file is missing"
    );
    crate::frontend::namespace::ProjectSymbols::from_plan(plan)?;
    plan.files
        .iter()
        .map(|file| {
            let source = std::fs::read_to_string(&file.path)
                .with_context(|| format!("reading {}", file.path.display()))?;
            let ast = crate::frontend::parser::Parser::with_source(
                crate::frontend::tokenizer::Tokenizer::new(&source),
                &file.path,
                &source,
            )
            .parse()?;
            Ok(CompilationUnit {
                source_file_id: file.id,
                ast,
            })
        })
        .collect()
}

/// The sole frontend-to-IR boundary. A future implementation must validate
/// analysis completeness, translate semantic types and identities into owned
/// IR records, and construct SSA before invoking IR verification. It must not
/// fall back to direct AST-to-backend emission.
pub fn lower_to_ir(_project: &AnalyzedProject) -> anyhow::Result<Ir> {
    anyhow::bail!("AnalyzedProject to SSA IR lowering is not implemented")
}

pub mod compiler_output;
#[cfg(test)]
mod test;
