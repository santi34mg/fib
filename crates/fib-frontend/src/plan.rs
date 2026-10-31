use std::{
    collections::HashSet,
    path::{Component, Path, PathBuf},
};

/// Identity within one build plan; independent of a file's path or aliases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FileId(pub u32);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FileName(pub String);

impl From<String> for FileName {
    fn from(value: String) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourceFile {
    pub id: FileId,
    pub name: FileName,
    pub path: PathBuf,
}

/// Package identity is independent of local dependency spellings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PackageId(pub u32);

/// Host-owned metadata; package acquisition and build-API registration are separate.
#[derive(Debug, Clone)]
pub struct PackageSpec {
    pub id: PackageId,
    /// Persistent acquisition identity, including version/revision (for example
    /// `registry:example.org/math@1.2.0`). Hosts must reuse it for diamond edges.
    /// Never derive this from a checkout path or a dependency alias.
    pub identity: String,
    pub source_root: PathBuf,
    pub dependencies: std::collections::BTreeMap<String, PackageId>,
    pub files: Vec<FileId>,
}

#[derive(Debug, Clone)]
pub struct FileRegion {
    pub package: PackageId,
    pub directory: PathBuf,
}

/// The complete set of files visible to name resolution.
#[derive(Debug, Clone, Default)]
pub struct NativeInputs {
    pub sources: Vec<PathBuf>,
    pub libraries: Vec<String>,
    pub library_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct BuildPlan {
    pub standard_library: crate::standard_library::StandardLibrary,
    pub native: NativeInputs,
    pub packages: Vec<PackageSpec>,
    pub files: Vec<SourceFile>,
    /// ID of the entry point file
    pub entry: FileId,
    pub output: PathBuf,
}

impl BuildPlan {
    pub fn with_standard_library(
        mut self,
        standard_library: crate::standard_library::StandardLibrary,
    ) -> Self {
        self.standard_library = standard_library;
        self
    }

    pub(crate) fn file_regions(
        &self,
    ) -> anyhow::Result<std::collections::HashMap<FileId, FileRegion>> {
        use std::collections::HashMap;
        if self.packages.is_empty() {
            return Ok(self
                .files
                .iter()
                .map(|file| {
                    (
                        file.id,
                        FileRegion {
                            package: PackageId(0),
                            directory: file.path.parent().unwrap_or(Path::new("")).to_owned(),
                        },
                    )
                })
                .collect());
        }
        let mut packages = HashSet::new();
        let mut identities = HashSet::new();
        for package in &self.packages {
            anyhow::ensure!(packages.insert(package.id), "duplicate package identity");
            anyhow::ensure!(
                !package.identity.is_empty()
                    && !package.identity.chars().any(char::is_control)
                    && identities.insert(&package.identity),
                "invalid or duplicate persistent package identity"
            );
            anyhow::ensure!(
                package.source_root.is_absolute(),
                "package source root must be absolute"
            );
            anyhow::ensure!(
                !package
                    .source_root
                    .components()
                    .any(|c| matches!(c, Component::ParentDir)),
                "package source root cannot contain '..'"
            );
        }
        let mut regions = HashMap::new();
        for package in &self.packages {
            for (name, id) in &package.dependencies {
                anyhow::ensure!(
                    !name.is_empty() && name != "root" && packages.contains(id),
                    "invalid dependency environment"
                );
            }
            for id in &package.files {
                let file = self
                    .files
                    .iter()
                    .find(|file| file.id == *id)
                    .ok_or_else(|| anyhow::anyhow!("package references an unknown source file"))?;
                let source_path = std::path::absolute(&file.path)?;
                anyhow::ensure!(
                    source_path.starts_with(&package.source_root)
                        && !file
                            .path
                            .components()
                            .any(|c| matches!(c, Component::ParentDir)),
                    "source lies outside its package source root"
                );
                anyhow::ensure!(
                    regions
                        .insert(
                            *id,
                            FileRegion {
                                package: package.id,
                                directory: source_path
                                    .parent()
                                    .unwrap_or(&package.source_root)
                                    .to_owned()
                            }
                        )
                        .is_none(),
                    "source belongs to multiple packages"
                );
            }
        }
        anyhow::ensure!(
            regions.len() == self.files.len(),
            "every source must belong to one package"
        );
        Ok(regions)
    }

    pub fn single_file(path: &Path) -> anyhow::Result<Self> {
        let path = path.canonicalize()?;
        anyhow::ensure!(path.is_file(), "source is not a file: {}", path.display());
        let name = path
            .file_stem()
            .and_then(|n| n.to_str())
            .ok_or_else(|| anyhow::anyhow!("source needs a UTF-8 file stem"))?
            .to_owned();
        let mut output = path.with_extension("");
        if cfg!(windows) {
            output.set_extension("exe");
        }
        anyhow::ensure!(
            output != path,
            "source must have an extension to choose a distinct output path"
        );
        Ok(Self {
            standard_library: crate::standard_library::StandardLibrary::default(),
            native: crate::plan::NativeInputs::default(),
            packages: Vec::new(),
            files: vec![SourceFile {
                id: FileId(0),
                name: FileName(name),
                path,
            }],
            entry: FileId(0),
            output,
        })
    }
}

#[cfg(test)]
mod package_metadata_tests {
    use super::*;

    fn plan() -> BuildPlan {
        BuildPlan {
            standard_library: crate::standard_library::StandardLibrary::disabled(),
            native: crate::plan::NativeInputs::default(),
            files: vec![SourceFile {
                id: FileId(0),
                name: "main".to_owned().into(),
                path: "/app/src/main.fib".into(),
            }],
            packages: vec![PackageSpec {
                id: PackageId(7),
                identity: "local:application".into(),
                source_root: "/app/src".into(),
                dependencies: Default::default(),
                files: vec![FileId(0)],
            }],
            entry: FileId(0),
            output: "/app/output".into(),
        }
    }

    #[test]
    fn package_membership_roots_and_dependencies_are_validated_before_analysis() {
        let good = plan();
        let regions = good.file_regions().unwrap();
        assert_eq!(regions[&FileId(0)].package, PackageId(7));
        for change in 0..6 {
            let mut bad = plan();
            match change {
                0 => bad.packages.push(bad.packages[0].clone()),
                1 => bad.packages[0].files.clear(),
                2 => bad.packages[0].files.push(FileId(1)),
                3 => bad.packages[0].source_root = "/other/src".into(),
                4 => {
                    bad.packages[0]
                        .dependencies
                        .insert("missing".into(), PackageId(999));
                }
                5 => bad.files[0].path = "/app/src/../elsewhere.fib".into(),
                _ => unreachable!(),
            }
            assert!(
                bad.file_regions().is_err(),
                "invalid package metadata case {change}"
            );
        }
    }
}
