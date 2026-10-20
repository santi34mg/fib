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
pub fn run_pipeline(plan: &BuildPlan, goal: PipelineGoal) -> anyhow::Result<CompilationOutput> {
    let project = run_frontend(plan)?;
    if goal == PipelineGoal::Check {
        return Ok(CompilationOutput::Checked);
    }
    let ir = lower_to_ir(&project)?;
    if goal == PipelineGoal::Ir {
        return Ok(CompilationOutput::Ir(ir));
    }
    let target = crate::backend::lowering::llvm_lower::host_target()?;
    let artifact = crate::backend::emit_llvm(&ir, &target)?;
    let output = if goal == PipelineGoal::Objects {
        plan.output.with_extension("o")
    } else {
        plan.output.clone()
    };
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));
    let parent = parent.canonicalize()?;
    let output = parent.join(
        output
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("output needs a file name"))?,
    );
    let resolved_output = output.canonicalize().unwrap_or_else(|_| output.clone());
    for source in &plan.files {
        anyhow::ensure!(
            source.path.canonicalize()? != resolved_output,
            "output would overwrite source {}",
            source.path.display()
        );
    }
    let temporary = tempfile::tempdir_in(&parent)?;
    let object = temporary.path().join("module.o");
    artifact.write_object(&object)?;
    if goal == PipelineGoal::Objects {
        std::fs::rename(&object, &output)?;
        return Ok(CompilationOutput::Objects(vec![output]));
    }
    let binary = temporary.path().join("program");
    let linker = std::process::Command::new("cc")
        .arg(&object)
        .arg("-o")
        .arg(&binary)
        .output()
        .context("invoking host linker cc")?;
    anyhow::ensure!(
        linker.status.success(),
        "linking failed: {}",
        String::from_utf8_lossy(&linker.stderr)
    );
    std::fs::rename(binary, &output)?;
    Ok(CompilationOutput::Binary(output))
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

/// Lower the supported scalar/conditional subset after independent analysis validation.
pub fn lower_to_ir(project: &AnalyzedProject) -> anyhow::Result<Ir> {
    lowering::lower(project)
}

mod lowering;

pub mod compiler_output;
#[cfg(test)]
mod test;
