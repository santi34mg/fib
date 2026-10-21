use std::{env, path::PathBuf};

use clap::Parser;

use crate::plan::PlanBuilder;

#[derive(Parser, Debug)]
#[command(name = "fib", about = "Integrated compilation tool")]
pub enum Command {
    #[command(name = "build")]
    Build {
        /// Compile one file without requiring build.fib.
        #[arg(long, value_name = "PATH")]
        single_file: Option<PathBuf>,
        #[arg(long, value_name = "PATH")]
        build_file: Option<PathBuf>,
    },

    #[command(name = "run")]
    Run,

    #[command(name = "test")]
    Test,
}

pub fn exec_command(command: Command, _env_vars: env::VarsOs) -> anyhow::Result<()> {
    match command {
        Command::Build {
            single_file: None,
            build_file: None,
        } => {
            anyhow::bail!("either single_file or build_file must be provided")
        }
        Command::Build {
            single_file: Some(_),
            build_file: Some(_),
        } => {
            anyhow::bail!("cannot provide single_file and build_file at the same time")
        }
        Command::Build {
            single_file: Some(path),
            build_file: None,
        } => {
            println!("Beginning compilation process...");
            let plan = crate::plan::BuildPlan::single_file(&path)?;
            let output = crate::driver::run_pipeline(&plan, crate::driver::PipelineGoal::Binary)?;
            if let crate::driver::compiler_output::CompilationOutput::Binary(path) = output {
                println!("{}", path.display());
            }
            Ok(())
        }
        Command::Build {
            single_file: None,
            build_file: Some(build_file),
        } => {
            println!("Beginning compilation process...");
            let path = build_file;
            if !path.is_file() {
                anyhow::bail!("build.fib not found in the current working directory");
            }
            let plan = PlanBuilder::from(path).compile_and_interpret_build_file()?;
            let output = crate::driver::run_pipeline(&plan, crate::driver::PipelineGoal::Binary)?;
            if let crate::driver::compiler_output::CompilationOutput::Binary(path) = output {
                println!("{}", path.display());
            }
            Ok(())
        }
        Command::Run => anyhow::bail!("run is not implemented"),
        Command::Test => anyhow::bail!("test is not implemented"),
    }
}
