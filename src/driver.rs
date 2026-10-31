use std::fmt::Display;

use anyhow::Context;

use crate::driver::compiler_output::CompilationOutput;
use crate::frontend::analyzer::AnalyzedProject;
use crate::ir::Ir;
use crate::plan::BuildPlan;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineGoal {
    Check,
    Ir,
    Objects,
    Binary,
}

impl Display for PipelineGoal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PipelineGoal::Check => write!(f, "check"),
            PipelineGoal::Ir => write!(f, "ir"),
            PipelineGoal::Objects => write!(f, "object files"),
            PipelineGoal::Binary => write!(f, "binary file"),
        }
    }
}

pub use fib_frontend::CompilationUnit;

/// Compile only the files explicitly enumerated and named by the plan.
pub fn run_pipeline(plan: &BuildPlan, goal: PipelineGoal) -> anyhow::Result<CompilationOutput> {
    let prepared = plan.prepare()?;
    let plan = &prepared;
    println!("Running pipeline with goal {}...", goal);
    let mut units = parse_files(plan)?;
    let mut project = analyze_units(plan, units.clone())?;
    let wrapper = entry::startup_wrapper(&project, plan)?;
    if goal == PipelineGoal::Check {
        return Ok(CompilationOutput::Checked);
    }
    let ir = if let Some((name, source)) = wrapper {
        let startup = crate::frontend::parser::Parser::with_source(
            crate::frontend::tokenizer::Tokenizer::new(&source),
            std::path::Path::new("<process startup>"),
            &source,
        )
        .parse()?;
        units
            .iter_mut()
            .find(|unit| unit.source_file_id == plan.entry)
            .expect("validated entry file")
            .ast
            .statements
            .extend(startup.statements);
        project = crate::frontend::analyzer::Analyzer::new(plan.clone(), units)?.analyze()?;
        crate::ir::lowering::lower_with_entry(&project, &name)?
    } else {
        crate::ir::lowering::lower(&project)?
    };
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
    for source in plan
        .files
        .iter()
        .map(|f| &f.path)
        .chain(&plan.native.sources)
    {
        anyhow::ensure!(
            source.canonicalize()? != resolved_output,
            "output would overwrite source {}",
            source.display()
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
    let mut objects = Vec::new();
    for (index, source) in plan.native.sources.iter().enumerate() {
        let output = temporary.path().join(format!("native-{index}.o"));
        let compiler = std::process::Command::new("cc")
            .arg("-std=c11")
            .arg("-fPIC")
            .arg("-c")
            .arg(source)
            .arg("-o")
            .arg(&output)
            .output()
            .context("invoking host C compiler cc")?;
        anyhow::ensure!(
            compiler.status.success(),
            "C compilation failed for {}: {}",
            source.display(),
            String::from_utf8_lossy(&compiler.stderr)
        );
        objects.push(output);
    }
    let mut command = std::process::Command::new("cc");
    command.arg(&object).args(&objects);
    for path in &plan.native.library_paths {
        command.arg("-L").arg(path);
    }
    for library in &plan.native.libraries {
        command.arg(format!("-l{library}"));
    }
    let linker = command
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

/// Prepare and strictly analyze a project without requiring an executable entry.
pub fn run_frontend(plan: &BuildPlan) -> anyhow::Result<AnalyzedProject> {
    let prepared = plan.prepare()?;
    let plan = &prepared;
    analyze_units(plan, parse_files(plan)?)
}

fn analyze_units(plan: &BuildPlan, units: Vec<CompilationUnit>) -> anyhow::Result<AnalyzedProject> {
    let project = crate::frontend::analyzer::Analyzer::new(plan.clone(), units)?.analyze()?;
    for warning in &project.warnings {
        eprintln!("{}", warning.render());
    }
    Ok(project)
}

/// Compile the isolated build file with the compiler-provided build API.
pub fn compile_build_file_to_ir(plan: &BuildPlan) -> anyhow::Result<Ir> {
    compile_build_file_to_ir_with_sources(plan, &crate::frontend::source::DiskSources)
}

pub fn compile_build_file_to_ir_with_sources(
    plan: &BuildPlan,
    sources: &dyn crate::frontend::source::SourceProvider,
) -> anyhow::Result<Ir> {
    let prepared = plan.prepare()?;
    let plan = &prepared;
    let project = crate::frontend::analyzer::Analyzer::new(
        plan.clone(),
        parse_files_with_sources(plan, sources)?,
    )?
    .with_synthetic_module(crate::builtins::SyntheticModule::BUILD_API)?
    .analyze()?;
    require_entry(&project, "build")?;
    crate::ir::lowering::lower(&project)
}

fn require_entry(project: &AnalyzedProject, name: &str) -> anyhow::Result<()> {
    let matches = project
        .units
        .iter()
        .filter(|unit| unit.source_file_id == project.entry)
        .flat_map(|unit| &unit.ast.statements)
        .filter_map(|statement| {
            let crate::frontend::ast::statement::StatementKind::FunctionDeclaration(function) =
                &statement.kind
            else {
                return None;
            };
            (function.signature.identifier == name).then_some(function)
        })
        .collect::<Vec<_>>();
    anyhow::ensure!(
        matches.len() == 1,
        "expected exactly one '{name}' entry function"
    );
    let entry = matches[0];
    anyhow::ensure!(
        entry.body.is_some() && !entry.is_extern && entry.signature.parameters.is_empty(),
        "entry function '{name}' must be a definition with no parameters"
    );
    let return_type = entry
        .signature
        .return_type
        .annotation
        .resolved
        .ok_or_else(|| anyhow::anyhow!("entry function '{name}' has no resolved return type"))?;
    anyhow::ensure!(
        matches!(
            project.types.get(project.types.canonical(return_type)?)?,
            crate::frontend::semantic_type::SemanticType::Builtin(
                crate::frontend::tokens::builtin::BuiltinType::Int32
            )
        ),
        "entry function '{name}' must return int32"
    );
    Ok(())
}

pub fn parse_files(plan: &BuildPlan) -> anyhow::Result<Vec<CompilationUnit>> {
    parse_files_with_sources(plan, &crate::frontend::source::DiskSources)
}

pub fn parse_files_with_sources(
    plan: &BuildPlan,
    sources: &dyn crate::frontend::source::SourceProvider,
) -> anyhow::Result<Vec<CompilationUnit>> {
    anyhow::ensure!(
        plan.files.iter().any(|file| file.id == plan.entry),
        "build-plan entry file is missing"
    );
    crate::frontend::namespace::ProjectSymbols::from_plan(plan)?;
    plan.files
        .iter()
        .map(|file| {
            let source = sources
                .read(&file.path)
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

pub mod compiler_output;
mod entry;
#[cfg(test)]
mod test;
