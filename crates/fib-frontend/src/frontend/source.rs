//! Source loading shared by command-line compilation and editor overlays.
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

pub trait SourceProvider {
    fn read(&self, path: &Path) -> std::io::Result<String>;
}

pub struct DiskSources;
impl SourceProvider for DiskSources {
    fn read(&self, path: &Path) -> std::io::Result<String> {
        std::fs::read_to_string(path)
    }
}

#[derive(Clone, Default)]
pub struct OverlaySources(pub HashMap<PathBuf, String>);
impl SourceProvider for OverlaySources {
    fn read(&self, path: &Path) -> std::io::Result<String> {
        self.0
            .get(path)
            .cloned()
            .map(Ok)
            .unwrap_or_else(|| DiskSources.read(path))
    }
}
