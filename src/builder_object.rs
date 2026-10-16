use std::path::PathBuf;

use anyhow::bail;

use crate::{
    driver::{self, compiler_output::CompilationOutput, run_pipeline},
    ir_interpreter,
};

const BUILD_FILE_NAME: &'static str = "build.fib";

/// BuilderObject uses the builder pattern to create an object that
/// guides the build process of the project.
pub struct BuilderObject {
    /// True if the build configuration is done
    pub build_stage: BuildStage,
    /// Main file that orchestrates the compilation process
    // note that PathBuf is owning
    build_file: Option<PathBuf>,
    /// Modules defined in the build file
    modules: Vec<PathBuf>,
    /// Optimization level passed to clang as `-O<level>`.
    /// Accepted: `0`, `1`, `2`, `3`, `s`, `z`.
    opt_level: Option<String>,
    /// Release mode (`--release`): skip debug runtime bounds checks.
    /// When false (default, debug) every `arr.[i]` / `arr.[a..b]` lowers to
    /// an explicit OOB check that reports to stderr and aborts.
    release: bool,
}

impl BuilderObject {
    pub fn new() -> Self {
        Self {
            build_stage: BuildStage::Configuration,
            build_file: None,
            modules: Vec::new(),
            // doesn't mean 0, just not asigned
            opt_level: None,
            // debug mode by default
            release: false,
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

    /// `compile_build_file` compiles the build file into IR and interprets it,
    /// leaving the BuilderObject ready to build
    pub fn compile_and_interpret_build_file(&mut self) -> anyhow::Result<&mut Self> {
        if let Some(p) = self.build_file.clone() {
            match driver::run_pipeline(&p, &self)? {
                CompilationOutput::Ir(ir) => {
                    // TODO: here we need to interpret the IR
                    //
                    // the reason we can't simply iterate over ir as a flat list
                    // is that comp time instructions might be within control flow
                    // so we need an interpreter to follow the build instructions
                    // correctly.
                    // so exciting :)
                    self.modules = ir_interpreter::interpret_ir(ir, self)?;
                    self.build_stage = BuildStage::IndividualIrCompilation;
                }
                _ => {
                    // if this ever happens we made some sort of mistake
                    bail!("build file compilation emmited ir or binary")
                }
            };
            Ok(self)
        } else {
            bail!("no build file has been added");
        }
    }

    /// `build` is a sink for the builder pattern, turns a `BuilderObject`
    /// into a project.
    /// Can return any of the `CompileOutputs` (Frontend, IR, Binary).
    pub fn build<'a>(&mut self) -> anyhow::Result<CompilationOutput<'a>> {
        let BuildStage::IndividualIrCompilation = self.build_stage else {
            bail!("builder is not ready yet")
        };
        for path in self.modules.iter() {
            if let CompilationOutput::ObjectFile(obj) = run_pipeline(&path, &self)? {
                std::fs::write(path.with_extension("obj"), obj);
            } else {
                unreachable!()
            }
        }
        self.build_stage = BuildStage::Linking;
        // TODO: linking
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
