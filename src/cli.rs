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

pub fn exec_command(_command: Command, _env_vars: env::VarsOs) -> anyhow::Result<()> {
    // Select a one-file plan for --single-file or interpret build.fib, then run the pipeline.
    todo!()
}
