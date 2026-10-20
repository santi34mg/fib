use std::path::{Path, PathBuf};

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

/// The complete set of files visible to name resolution.
#[derive(Debug, Clone)]
pub struct BuildPlan {
    pub files: Vec<SourceFile>,
    /// ID of the entry point file
    pub entry: FileId,
    pub output: PathBuf,
}

impl BuildPlan {
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
