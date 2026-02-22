use clap::Parser;
use std::{path::PathBuf, process};

use crate::driver;

/// Minimal CLI for the `fiberc` binary: compile a single module file.
#[derive(Parser, Debug)]
#[command(name = "fiberc", about = "Compile a single Fiber module")]
pub struct Args {
    /// Path to the Fiber source file to compile
    #[arg(value_name = "FILE")]
    pub file: PathBuf,

    /// Enable debug output
    #[arg(short, long, default_value_t = false)]
    pub is_debug: bool,
}

pub fn parse_args() -> Args {
    Args::parse()
}

pub fn exec_command(args: Args) {
    if let Err(e) = driver::compile_project(&args.file, args.is_debug) {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}
