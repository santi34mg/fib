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

#[derive(Debug, Clone)]
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

pub fn run_frontend(_plan: &BuildPlan) -> anyhow::Result<AnalyzedProject> {
    // Parse every named file, collect declarations, then resolve names and types.
    anyhow::bail!("frontend orchestration is not implemented")
}

pub fn parse_files(_plan: &BuildPlan) -> anyhow::Result<Vec<CompilationUnit>> {
    // Read, tokenize, and parse every source file listed in the build plan.
    anyhow::bail!("build-plan file parsing is not implemented")
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
