use std::collections::HashMap;

use crate::build_plan::{BuildPlan, FileId, FileName};
use crate::diagnostics::Span;

// Keep the existing namespace import path while identities live independently
// of both spelling and namespace storage.
pub use crate::frontend::identifier::{DeclId, Entity, LocalId, ResolvedName};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Private,
    Public,
}

#[derive(Debug, Clone)]
pub struct Declaration {
    pub name: String,
    pub visibility: Visibility,
    pub span: Span,
    pub alias_target: Option<Entity>,
}

/// Declarations owned by a source file, independent of its lexical scopes.
#[derive(Debug, Clone)]
pub struct Namespace {
    pub file: FileId,
    pub declarations: Vec<Declaration>,
    pub names: HashMap<String, DeclId>,
}

/// Only files explicitly named in the build plan may be selected by a path.
#[derive(Debug, Clone)]
pub struct ProjectSymbols {
    pub files_by_name: HashMap<FileName, FileId>,
    pub namespaces: HashMap<FileId, Namespace>,
}

impl ProjectSymbols {
    pub fn from_plan(_plan: &BuildPlan) -> anyhow::Result<Self> {
        // Register only plan files; reject duplicate IDs and ambiguous names.
        anyhow::bail!("build-plan symbol registration is not implemented")
    }

    pub fn resolve_path(&self, _from: FileId, _segments: &[String]) -> anyhow::Result<Entity> {
        // Follow file namespaces and comp aliases, enforcing visibility and
        // rejecting alias-only cycles. This must never discover files on disk.
        anyhow::bail!("file path and alias resolution is not implemented")
    }
}
