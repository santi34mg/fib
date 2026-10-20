use std::{env, path::PathBuf};

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "fib", about = "Integrated compilation tool")]
pub enum Command {
    #[command(name = "build")]
    Build {
        /// Compile one file without requiring build.fib.
        #[arg(long, value_name = "PATH")]
        single_file: Option<PathBuf>,
    },

    #[command(name = "run")]
    Run,

    #[command(name = "test")]
    Test,
}

pub fn exec_command(command: Command, _env_vars: env::VarsOs) -> anyhow::Result<()> {
    match command {
        Command::Build {
            single_file: Some(path),
        } => {
            let plan = crate::build_plan::BuildPlan::single_file(&path)?;
            let output = crate::driver::run_pipeline(&plan, crate::driver::PipelineGoal::Binary)?;
            if let crate::driver::compiler_output::CompilationOutput::Binary(path) = output {
                println!("{}", path.display());
            }
            Ok(())
        }
        Command::Build { single_file: None } => anyhow::bail!(
            "build.fib interpretation is not implemented; use build --single-file PATH"
        ),
        Command::Run => anyhow::bail!("run is not implemented"),
        Command::Test => anyhow::bail!("test is not implemented"),
    }
}
