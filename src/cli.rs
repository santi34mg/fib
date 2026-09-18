use clap::Parser;
use std::{path::PathBuf, process};

use crate::driver::{CompilationOptions, CompileOutput, DriverError, EmitKind};

fn parse_emit_kind(s: &str) -> Result<EmitKind, String> {
    s.parse()
}

/// Compile a single Fib module.
///
/// The default (`--emit=bin`) produces a native binary at `out/<stem>` (or
/// `--output`). Frontend-only stages (`lex|parse|typed`, `--check`) run
/// without LLVM or clang, which makes them suitable for CI and editor
/// integration.
#[derive(Parser, Debug)]
#[command(name = "fibc", about = "Compile a single Fib module")]
pub struct Args {
    /// Path to the Fib source file to compile
    #[arg(value_name = "FILE")]
    pub file: PathBuf,

    /// Extra directories to search when resolving imports (repeatable).
    /// The entry file's directory is always searched first.
    #[arg(short = 'I', long = "include-path", value_name = "DIR")]
    pub include_path: Vec<PathBuf>,

    /// Output binary path (for `--emit=bin`) or `.ll` path (for `--emit=llvm`).
    /// Defaults to `out/<stem>` / `out/<stem>.ll`.
    #[arg(short = 'o', long = "output", value_name = "FILE")]
    pub output: Option<PathBuf>,

    /// Stop after this stage: lex|parse|typed|llvm|bin.
    #[arg(long = "emit", value_parser = parse_emit_kind, default_value = "bin", value_name = "KIND")]
    pub emit: EmitKind,

    /// Run the frontend only (lex + parse + analyze), no codegen.
    /// Equivalent to `--emit=typed` without needing LLVM.
    #[arg(long = "check")]
    pub check: bool,

    /// Keep the intermediate LLVM IR next to the binary
    /// (`<output>.ll`). Ignored for `--emit=llvm` (always kept).
    #[arg(long = "emit-llvm")]
    pub emit_llvm: bool,

    /// Explicit path for the intermediate LLVM IR (implies keeping it).
    #[arg(long = "llvm-out", value_name = "FILE")]
    pub llvm_out: Option<PathBuf>,

    /// C compiler used for linking. Defaults to `$CC`, else `clang-17`, else
    /// `clang`.
    #[arg(long = "cc", value_name = "CC")]
    pub cc: Option<PathBuf>,

    /// Optimization level passed to clang as `-O<LEVEL>`.
    /// Accepted: 0, 1, 2, 3, s, z.
    #[arg(short = 'O', long = "opt-level", value_name = "LEVEL")]
    pub opt_level: Option<String>,
}

impl From<Args> for CompilationOptions {
    fn from(args: Args) -> Self {
        CompilationOptions {
            project_path: args.file,
            source_override: None,
            include_paths: args.include_path,
            output: args.output,
            emit: args.emit,
            check_only: args.check,
            emit_llvm: args.emit_llvm,
            llvm_out: args.llvm_out,
            cc: args.cc,
            opt_level: args.opt_level,
        }
    }
}

pub fn parse_args() -> Args {
    Args::parse()
}

fn print_frontend(emit: EmitKind, frontend: &crate::driver::FrontendResponse, check_only: bool) {
    match emit {
        EmitKind::Lex => {
            for tok in &frontend.tokens {
                println!("{:?}", tok);
            }
        }
        EmitKind::Parse => {
            println!("{:#?}", frontend.ast);
        }
        EmitKind::Typed => {
            if check_only {
                println!(
                    "OK: {} ({} declaration(s), {} module(s))",
                    frontend.filename,
                    frontend.typed_program.declarations.len(),
                    frontend.resolved_modules.len()
                );
            } else {
                println!("{:#?}", frontend.typed_program);
            }
        }
        _ => {}
    }
}

fn run(args: Args) -> Result<(), DriverError> {
    let check_only = args.check;
    let opts = CompilationOptions::from(args);
    match crate::driver::compile(&opts)? {
        CompileOutput::Frontend { emit, frontend } => {
            print_frontend(emit, &frontend, check_only);
            Ok(())
        }
        CompileOutput::Llvm { path } => {
            println!("Wrote LLVM IR to {}", path.display());
            Ok(())
        }
        CompileOutput::Binary { binary, llvm_ir } => {
            println!("Built binary: {}", binary.display());
            if let Some(ll) = llvm_ir {
                println!("Kept LLVM IR: {}", ll.display());
            }
            Ok(())
        }
    }
}

pub fn exec_command(args: Args) {
    if let Err(e) = run(args) {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn cli_defaults_to_bin_and_out_stem() {
        let args = Args::try_parse_from(["fibc", "main.fib"]).expect("parse");
        assert_eq!(args.emit, EmitKind::Bin);
        assert!(!args.check);
        assert_eq!(args.output, None);
        let opts = CompilationOptions::from(args);
        assert_eq!(opts.effective_emit(), EmitKind::Bin);
    }

    #[test]
    fn cli_parses_all_new_flags() {
        let args = Args::try_parse_from([
            "fibc",
            "main.fib",
            "-I",
            "std",
            "-o",
            "build/prog",
            "--emit",
            "llvm",
            "--emit-llvm",
            "--llvm-out",
            "build/prog.ll",
            "--cc",
            "clang",
            "-O",
            "2",
        ])
        .expect("parse");
        assert_eq!(args.include_path, vec![PathBuf::from("std")]);
        assert_eq!(args.output, Some(PathBuf::from("build/prog")));
        assert_eq!(args.emit, EmitKind::Llvm);
        assert!(args.emit_llvm);
        assert_eq!(args.llvm_out, Some(PathBuf::from("build/prog.ll")));
        assert_eq!(args.cc, Some(PathBuf::from("clang")));
        assert_eq!(args.opt_level.as_deref(), Some("2"));
    }

    #[test]
    fn cli_check_overrides_emit() {
        let args = Args::try_parse_from(["fibc", "main.fib", "--check"]).expect("parse");
        let opts = CompilationOptions::from(args);
        assert_eq!(opts.effective_emit(), EmitKind::Typed);
    }

    #[test]
    fn cli_rejects_bad_emit() {
        assert!(Args::try_parse_from(["fibc", "main.fib", "--emit", "nope"]).is_err());
    }
}
