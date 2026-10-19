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
    pub fn single_file(_path: &Path) -> anyhow::Result<Self> {
        // Name the input file and make it the only file visible to resolution.
        todo!()
    }
}
