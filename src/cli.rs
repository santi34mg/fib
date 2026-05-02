use clap::Parser;
use std::{path::PathBuf, process};

use crate::driver::{self, CompilationOptions};

/// Minimal CLI for the `fibc` binary: compile a single module file.
#[derive(Parser, Debug)]
#[command(name = "fibc", about = "Compile a single Fiber module")]
pub struct Args {
    /// Path to the Fiber source file to compile
    #[arg(value_name = "FILE")]
    pub file: PathBuf,
    #[arg(short = 'I')]
    pub include_path: Vec<PathBuf>,
}

pub fn parse_args() -> Args {
    Args::parse()
}

#[cfg(feature = "llvm")]
pub fn exec_command(args: Args) {
    let compilation_options = CompilationOptions::new(args);
    if let Err(e) = driver::compile(compilation_options) {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}
