use std::{
    collections::HashSet,
    path::{Component, Path, PathBuf},
};

use crate::{driver::compile_build_file_to_ir_with_sources, ir_interpreter::interpret_ir};

pub use fib_frontend::plan::*;

/// BuilderObject uses the builder pattern to create an object that
/// guides the build process of the project.
pub struct PlanBuilder {
    pub standard_library: crate::standard_library::StandardLibrary,
    /// True if the build configuration is done
    pub build_stage: BuildStage,
    /// Main file that orchestrates the compilation process
    // note that PathBuf is owning
    build_file: PathBuf,
    /// Modules defined in the build file
    pub modules: Vec<PathBuf>,
    pub native: NativeInputs,
    /// Logical entry source selected by the interpreted build file.
    pub(crate) entry_name: Option<String>,
    /// Output path selected by the interpreted build file.
    pub(crate) output: Option<PathBuf>,
    /// Optimization level passed to clang as `-O<level>`.
    /// Accepted: `0`, `1`, `2`, `3`, `s`, `z`.
    _opt_level: Option<String>,
}

impl From<PathBuf> for PlanBuilder {
    fn from(value: PathBuf) -> Self {
        Self {
            standard_library: crate::standard_library::StandardLibrary::default(),
            build_stage: BuildStage::Configuration,
            build_file: value,
            modules: Vec::new(),
            native: NativeInputs::default(),
            entry_name: None,
            output: None,
            _opt_level: None,
        }
    }
}

impl PlanBuilder {
    pub fn with_standard_library(
        mut self,
        standard_library: crate::standard_library::StandardLibrary,
    ) -> Self {
        self.standard_library = standard_library;
        self
    }

    /// Compile and interpret the selected build file, then atomically finalize its
    /// host-owned state into the immutable plan consumed by the normal pipeline.
    pub fn compile_and_interpret_build_file(self) -> anyhow::Result<BuildPlan> {
        self.compile_and_interpret_build_file_with_sources(&crate::frontend::source::DiskSources)
    }

    pub fn compile_and_interpret_build_file_with_sources(
        mut self,
        sources: &dyn crate::frontend::source::SourceProvider,
    ) -> anyhow::Result<BuildPlan> {
        let build_file = self.build_file.canonicalize().map_err(|error| {
            anyhow::anyhow!(
                "cannot resolve build file {}: {error}",
                self.build_file.display()
            )
        })?;
        anyhow::ensure!(
            build_file.is_file(),
            "build file is not a file: {}",
            build_file.display()
        );
        let project_root = build_file
            .parent()
            .ok_or_else(|| anyhow::anyhow!("build file has no project directory"))?
            .to_path_buf();

        self.build_stage = BuildStage::BuildFileCompilation;
        let mut bootstrap = BuildPlan::single_file(&build_file)?;
        bootstrap.standard_library = self.standard_library.clone();
        let ir = compile_build_file_to_ir_with_sources(&bootstrap, sources)?;
        interpret_ir(ir, &mut self)?;
        self.finalize(project_root, build_file)
    }

    fn finalize(mut self, project_root: PathBuf, build_file: PathBuf) -> anyhow::Result<BuildPlan> {
        anyhow::ensure!(
            !self.modules.is_empty(),
            "build file did not add any source files"
        );
        let entry_name = self
            .entry_name
            .take()
            .ok_or_else(|| anyhow::anyhow!("build file did not select an entry source"))?;
        anyhow::ensure!(!entry_name.is_empty(), "entry source name cannot be empty");
        let output = self
            .output
            .take()
            .ok_or_else(|| anyhow::anyhow!("build file did not select an output path"))?;
        let output = resolve_output(&project_root, &output)?;

        let mut names = HashSet::new();
        let mut paths = HashSet::new();
        let mut files = Vec::with_capacity(self.modules.len());
        for (index, declared_path) in self.modules.into_iter().enumerate() {
            let path = resolve_source(&project_root, &declared_path)?;
            anyhow::ensure!(
                path != build_file,
                "build file cannot also be a project source: {}",
                path.display()
            );
            anyhow::ensure!(
                paths.insert(path.clone()),
                "duplicate source path: {}",
                path.display()
            );
            let name = path
                .file_stem()
                .and_then(|value| value.to_str())
                .ok_or_else(|| {
                    anyhow::anyhow!("source needs a UTF-8 file stem: {}", path.display())
                })?
                .to_owned();
            anyhow::ensure!(
                !name.is_empty() && !name.contains("::"),
                "invalid source name '{name}' derived from {}",
                path.display()
            );
            anyhow::ensure!(names.insert(name.clone()), "duplicate source name '{name}'");
            let index = u32::try_from(index)
                .map_err(|_| anyhow::anyhow!("build plan file ID space exhausted"))?;
            files.push(SourceFile {
                id: FileId(index),
                name: FileName(name),
                path,
            });
        }

        let entry = files
            .iter()
            .find(|file| file.name.0 == entry_name)
            .map(|file| file.id)
            .ok_or_else(|| {
                anyhow::anyhow!("entry source '{entry_name}' was not added to the build plan")
            })?;
        anyhow::ensure!(
            output != build_file && files.iter().all(|file| file.path != output),
            "output would overwrite a source or build file: {}",
            output.display()
        );
        let mut native = self.native.clone();
        native.sources = native
            .sources
            .iter()
            .map(|p| resolve_source(&project_root, p))
            .collect::<anyhow::Result<_>>()?;
        native.library_paths = native
            .library_paths
            .iter()
            .map(|p| {
                let path = relative_path(&project_root, p, "library search")?.canonicalize()?;
                anyhow::ensure!(
                    path.starts_with(&project_root) && path.is_dir(),
                    "invalid library search directory"
                );
                Ok(path)
            })
            .collect::<anyhow::Result<_>>()?;
        anyhow::ensure!(
            native
                .sources
                .iter()
                .all(|p| *p != output && *p != build_file),
            "output would overwrite native source"
        );
        anyhow::ensure!(
            native
                .libraries
                .iter()
                .all(|s| !s.is_empty() && !s.starts_with('-') && !s.contains(['/', '\\', '\0'])),
            "invalid native library name"
        );
        self.build_stage = BuildStage::IndividualIrCompilation;
        Ok(BuildPlan {
            standard_library: self.standard_library,
            native,
            packages: Vec::new(),
            files,
            entry,
            output,
        })
    }
}

fn relative_path(root: &Path, path: &Path, kind: &str) -> anyhow::Result<PathBuf> {
    anyhow::ensure!(!path.as_os_str().is_empty(), "{kind} path cannot be empty");
    anyhow::ensure!(!path.is_absolute(), "{kind} path must be project-relative");
    let mut resolved = root.to_path_buf();
    for component in path.components() {
        match component {
            Component::Normal(part) => resolved.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                anyhow::bail!(
                    "{kind} path cannot escape the project root: {}",
                    path.display()
                )
            }
        }
    }
    Ok(resolved)
}

fn resolve_source(root: &Path, declared: &Path) -> anyhow::Result<PathBuf> {
    let joined = relative_path(root, declared, "source")?;
    let path = joined
        .canonicalize()
        .map_err(|error| anyhow::anyhow!("cannot resolve source {}: {error}", joined.display()))?;
    anyhow::ensure!(path.starts_with(root), "source escapes the project root");
    anyhow::ensure!(path.is_file(), "source is not a file: {}", path.display());
    Ok(path)
}

fn resolve_output(root: &Path, declared: &Path) -> anyhow::Result<PathBuf> {
    let output = relative_path(root, declared, "output")?;
    let file_name = output
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("output path needs a file name"))?;
    let parent = output
        .parent()
        .ok_or_else(|| anyhow::anyhow!("output path has no parent directory"))?
        .canonicalize()
        .map_err(|error| {
            anyhow::anyhow!(
                "cannot resolve output directory {}: {error}",
                output.parent().unwrap().display()
            )
        })?;
    anyhow::ensure!(parent.starts_with(root), "output escapes the project root");
    Ok(parent.join(file_name))
}

pub enum BuildStage {
    Configuration,
    BuildFileCompilation,
    IndividualIrCompilation,
    Linking,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_flow_build_sample_finalizes_plan() {
        let directory = tempfile::tempdir().unwrap();
        let build_file = directory.path().join("build.fib");
        std::fs::write(
            &build_file,
            include_str!("../samples/build_control_flow/build.fib"),
        )
        .unwrap();
        std::fs::write(
            directory.path().join("main.fib"),
            "fn main() int32 { return 0; }",
        )
        .unwrap();
        let plan = PlanBuilder::from(build_file)
            .with_standard_library(crate::standard_library::StandardLibrary::disabled())
            .compile_and_interpret_build_file()
            .unwrap();
        assert_eq!(plan.files.len(), 1);
        assert_eq!(
            plan.output,
            directory.path().canonicalize().unwrap().join("app")
        );
    }

    #[test]
    fn interpreted_build_file_finalizes_deterministic_project_plan() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let build_file = root.join("build.fib");
        let main = root.join("main.fib");
        let util = root.join("util.fib");
        std::fs::write(&main, "fn main() int32 { return 0; }").unwrap();
        std::fs::write(&util, "fn value() int32 { return 1; }").unwrap();
        std::fs::create_dir(root.join("build")).unwrap();
        std::fs::write(
            &build_file,
            r#"fn build() int32 {
                build_api::add_source("main.fib"[..]);
                build_api::add_source("util.fib"[..]);
                build_api::set_entry("main"[..]);
                build_api::set_output("build/app"[..]);
                return 0;
            }"#,
        )
        .unwrap();

        let plan = PlanBuilder::from(build_file)
            .with_standard_library(crate::standard_library::StandardLibrary::disabled())
            .compile_and_interpret_build_file()
            .unwrap();
        assert_eq!(plan.files.len(), 2);
        assert_eq!(plan.files[0].id, FileId(0));
        assert_eq!(plan.files[0].name, FileName("main".into()));
        assert_eq!(plan.files[0].path, main.canonicalize().unwrap());
        assert_eq!(plan.files[1].id, FileId(1));
        assert_eq!(plan.files[1].name, FileName("util".into()));
        assert_eq!(plan.entry, FileId(0));
        assert_eq!(plan.output, root.canonicalize().unwrap().join("build/app"));
    }

    fn builder(root: &Path, modules: &[&str], entry: &str, output: &str) -> PlanBuilder {
        let build_file = root.join("build.fib");
        std::fs::write(&build_file, "").unwrap();
        let mut builder = PlanBuilder::from(build_file);
        builder.modules = modules.iter().map(PathBuf::from).collect();
        builder.entry_name = Some(entry.into());
        builder.output = Some(output.into());
        builder
    }

    #[test]
    fn finalization_rejects_duplicate_names_missing_entry_and_escaping_paths() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        std::fs::create_dir(root.join("a")).unwrap();
        std::fs::create_dir(root.join("b")).unwrap();
        std::fs::write(root.join("a/main.fib"), "").unwrap();
        std::fs::write(root.join("b/main.fib"), "").unwrap();

        let duplicate = builder(&root, &["a/main.fib", "b/main.fib"], "main", "app")
            .finalize(root.clone(), root.join("build.fib"))
            .unwrap_err();
        assert!(duplicate.to_string().contains("duplicate source name"));

        let missing = builder(&root, &["a/main.fib"], "other", "app")
            .finalize(root.clone(), root.join("build.fib"))
            .unwrap_err();
        assert!(missing.to_string().contains("was not added"));

        let escaping = builder(&root, &["../outside.fib"], "outside", "app")
            .finalize(root.clone(), root.join("build.fib"))
            .unwrap_err();
        assert!(escaping.to_string().contains("cannot escape"));
    }

    #[test]
    fn finalization_rejects_output_overwriting_a_source() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        std::fs::write(root.join("main.fib"), "").unwrap();
        let error = builder(&root, &["main.fib"], "main", "main.fib")
            .finalize(root.clone(), root.join("build.fib"))
            .unwrap_err();
        assert!(error.to_string().contains("overwrite"));
    }
}
