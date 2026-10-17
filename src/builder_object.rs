use std::path::PathBuf;

use crate::{build_plan::BuildPlan, driver::compiler_output::CompilationOutput};

const BUILD_FILE_NAME: &str = "build.fib";

/// BuilderObject uses the builder pattern to create an object that
/// guides the build process of the project.
pub struct BuilderObject {
    /// True if the build configuration is done
    pub build_stage: BuildStage,
    /// Main file that orchestrates the compilation process
    // note that PathBuf is owning
    build_file: Option<PathBuf>,
    /// Modules defined in the build file
    _modules: Vec<PathBuf>,
    /// Optimization level passed to clang as `-O<level>`.
    /// Accepted: `0`, `1`, `2`, `3`, `s`, `z`.
    _opt_level: Option<String>,
    /// Release mode (`--release`): skip debug runtime bounds checks.
    /// When false (default, debug) every `arr.[i]` / `arr.[a..b]` lowers to
    /// an explicit OOB check that reports to stderr and aborts.
    _release: bool,
}

impl Default for BuilderObject {
    fn default() -> Self {
        Self::new()
    }
}

impl BuilderObject {
    pub fn new() -> Self {
        Self {
            build_stage: BuildStage::Configuration,
            build_file: None,
            _modules: Vec::new(),
            // doesn't mean 0, just not asigned
            _opt_level: None,
            // debug mode by default
            _release: false,
        }
    }

    pub fn add_build_file(&mut self) -> anyhow::Result<&mut Self> {
        let pwd = std::env::current_dir()?;
        let read_dir = std::fs::read_dir(pwd)?;
        for file in read_dir {
            let file = file?;
            let path = file.path();
            if *path == *BUILD_FILE_NAME {
                self.build_file = Some(path);
            }
        }
        self.build_stage = BuildStage::BuildFileCompilation;
        Ok(self)
    }

    pub fn compile_and_interpret_build_file(&mut self) -> anyhow::Result<&mut Self> {
        // Compile build.fib in isolation and interpret its IR into a build plan.
        todo!()
    }

    pub fn project_plan(&mut self) -> anyhow::Result<BuildPlan> {
        // Interpret build.fib when present; otherwise require --single-file.
        todo!()
    }

    pub fn build(&mut self) -> anyhow::Result<CompilationOutput> {
        // Produce a plan and compile its named files through run_pipeline.
        todo!()
    }

    pub fn run(self) -> anyhow::Result<()> {
        todo!()
    }
}

pub enum BuildStage {
    Configuration,
    BuildFileCompilation,
    IndividualIrCompilation,
    Linking,
}
