use std::{env, path::PathBuf};

use anyhow::Context;
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
        /// Use this build file instead of <cwd>/build.fib.
        #[arg(long, value_name = "PATH")]
        build_file: Option<PathBuf>,
        /// Installed version-matched standard-library sysroot.
        #[arg(long, value_name = "PATH", conflicts_with = "no_std")]
        sysroot: Option<PathBuf>,
        /// Compile without the standard-library dependency or its prelude.
        #[arg(long)]
        no_std: bool,
        #[arg(long = "c-source", value_name = "PATH")]
        c_sources: Vec<PathBuf>,
        #[arg(short = 'l', long = "link-lib", value_name = "LIB")]
        libraries: Vec<String>,
        #[arg(short = 'L', long = "library-path", value_name = "PATH")]
        library_paths: Vec<PathBuf>,
    },

    /// Export unrestricted public top-level symbols as JSON on stdout.
    #[command(name = "export-symbols")]
    ExportSymbols {
        #[arg(
            long,
            value_name = "PATH",
            required_unless_present = "build_file",
            conflicts_with = "build_file"
        )]
        single_file: Option<PathBuf>,
        #[arg(long, value_name = "PATH", required_unless_present = "single_file")]
        build_file: Option<PathBuf>,
        #[arg(long, value_name = "PATH", conflicts_with = "no_std")]
        sysroot: Option<PathBuf>,
        #[arg(long)]
        no_std: bool,
    },

    #[command(name = "run")]
    Run,

    #[command(name = "test")]
    Test,
}

pub fn exec_command(command: Command, _env_vars: env::VarsOs) -> anyhow::Result<()> {
    match command {
        Command::ExportSymbols {
            single_file,
            build_file,
            sysroot,
            no_std,
        } => {
            let mut standard_library = if no_std {
                crate::standard_library::StandardLibrary::disabled()
            } else {
                crate::standard_library::StandardLibrary::default()
            };
            standard_library.sysroot = sysroot;
            let mut plan = match (single_file, build_file) {
                (Some(path), None) => crate::plan::BuildPlan::single_file(&path)?,
                (None, Some(path)) => PlanBuilder::from(path)
                    .with_standard_library(standard_library.clone())
                    .compile_and_interpret_build_file()?,
                _ => anyhow::bail!("provide exactly one of --single-file or --build-file"),
            };
            plan.standard_library = standard_library;
            let project = crate::driver::run_frontend(&plan)?;
            let document = crate::symbol_export::public_symbols(&project)?;
            use std::io::Write;
            let mut stdout = std::io::stdout().lock();
            serde_json::to_writer_pretty(&mut stdout, &document)?;
            writeln!(stdout)?;
            Ok(())
        }
        Command::Build {
            single_file,
            build_file,
            sysroot,
            no_std,
            c_sources,
            libraries,
            library_paths,
        } => {
            let standard_library = if no_std {
                crate::standard_library::StandardLibrary::disabled()
            } else {
                let mut library = crate::standard_library::StandardLibrary::default();
                library.sysroot = sysroot;
                library
            };
            let mut plan = match (single_file, build_file) {
                (Some(path), None) => crate::plan::BuildPlan::single_file(&path)?,
                (None, build_file) => {
                    let path = match build_file {
                        Some(path) => path,
                        None => {
                            let path = env::current_dir()?.join("build.fib");
                            path.canonicalize().with_context(|| {
                                format!(
                                    "could not discover build file {}; create build.fib or use --build-file PATH or --single-file PATH",
                                    path.display()
                                )
                            })?
                        }
                    };
                    let mut builder = PlanBuilder::from(path);
                    builder.standard_library = standard_library.clone();
                    builder.compile_and_interpret_build_file()?
                }
                (Some(_), Some(_)) => {
                    anyhow::bail!("cannot provide single_file and build_file at the same time")
                }
            };
            plan.standard_library = standard_library;
            plan.native.sources.extend(
                c_sources
                    .into_iter()
                    .map(|p| p.canonicalize())
                    .collect::<std::io::Result<Vec<_>>>()?,
            );
            plan.native.libraries.extend(libraries);
            plan.native.library_paths.extend(library_paths);
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
