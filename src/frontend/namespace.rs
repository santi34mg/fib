use std::collections::{HashMap, HashSet};

use crate::build_plan::{BuildPlan, FileId, FileName};
use crate::diagnostics::{CompilerError, ErrorKind, Span};

pub use crate::frontend::identifier::{DeclId, Entity, LocalId, ResolvedName};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Private,
    Public,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclarationKind {
    Value,
    Type,
    Alias,
}

#[derive(Debug, Clone)]
pub struct Declaration {
    pub name: String,
    pub visibility: Visibility,
    pub span: Span,
    pub kind: DeclarationKind,
    pub alias_target: Option<Entity>,
}

/// File declarations followed by private local type slots. Only `names` exports
/// addressable file bindings; local type slots are reached through lexical scopes.
#[derive(Debug, Clone)]
pub struct Namespace {
    pub file_id: FileId,
    pub declarations: Vec<Declaration>,
    pub names: HashMap<String, DeclId>,
}

#[derive(Debug, Clone)]
pub struct ProjectSymbols {
    pub files_by_name: HashMap<FileName, FileId>,
    pub namespaces: HashMap<FileId, Namespace>,
}

pub(crate) fn name_error(message: impl Into<String>) -> anyhow::Error {
    CompilerError::new(ErrorKind::Name, message.into()).into()
}

impl ProjectSymbols {
    pub fn from_plan(plan: &BuildPlan) -> anyhow::Result<Self> {
        let mut ids = HashSet::new();
        let mut files_by_name = HashMap::new();
        for file in &plan.files {
            if !ids.insert(file.id) {
                return Err(name_error("duplicate file IDs in build plan"));
            }
            if file.name.0.is_empty() || file.name.0.split("::").any(str::is_empty) {
                return Err(name_error(format!("invalid file name '{}'", file.name.0)));
            }
            if files_by_name.insert(file.name.clone(), file.id).is_some() {
                return Err(name_error(format!("duplicate file name '{}'", file.name.0)));
            }
        }
        Ok(Self {
            files_by_name,
            namespaces: HashMap::new(),
        })
    }

    pub(crate) fn declaration(&self, id: DeclId) -> anyhow::Result<&Declaration> {
        self.namespaces
            .get(&id.file_id)
            .and_then(|ns| ns.declarations.get(id.index as usize))
            .ok_or_else(|| name_error(format!("invalid declaration {id:?}")))
    }

    pub(crate) fn reserved_file_root(&self, name: &str) -> bool {
        self.files_by_name
            .keys()
            .any(|file| file.0.split("::").next() == Some(name))
    }

    pub fn resolve_path(&self, from: FileId, segments: &[String]) -> anyhow::Result<Entity> {
        self.resolve_path_using(from, segments, &mut |entity| self.canonical(entity))
    }

    /// Shared path selection. The callback resolves either pending source aliases
    /// during the dependency walk or already recorded aliases during AST traversal.
    pub(crate) fn resolve_path_using(
        &self,
        from: FileId,
        segments: &[String],
        normalize: &mut impl FnMut(Entity) -> anyhow::Result<Entity>,
    ) -> anyhow::Result<Entity> {
        if segments.is_empty() {
            return Err(name_error("empty qualified name"));
        }
        let full = segments.join("::");
        // Unqualified declarations take precedence over a same-spelled file.
        let local = self
            .namespaces
            .get(&from)
            .and_then(|ns| ns.names.get(&segments[0]))
            .copied();
        if segments.len() == 1
            && let Some(id) = local
        {
            return normalize(Entity::Declaration(id));
        }
        if let Some((file, count)) = self.file_prefix(segments) {
            return self.follow_path(from, Entity::File(file), &segments[count..], normalize);
        }
        if let Some(id) = local {
            return self.follow_path(
                from,
                normalize(Entity::Declaration(id))?,
                &segments[1..],
                normalize,
            );
        }
        Err(name_error(format!(
            "unresolved file or identifier '{full}'"
        )))
    }

    /// Longest canonical file prefix, shared with lexical qualified lookup.
    pub(crate) fn file_prefix(&self, segments: &[String]) -> Option<(FileId, usize)> {
        (1..=segments.len()).rev().find_map(|count| {
            self.files_by_name
                .get(&FileName(segments[..count].join("::")))
                .map(|file| (*file, count))
        })
    }

    pub(crate) fn follow_path(
        &self,
        from: FileId,
        mut entity: Entity,
        segments: &[String],
        normalize: &mut impl FnMut(Entity) -> anyhow::Result<Entity>,
    ) -> anyhow::Result<Entity> {
        for segment in segments {
            let Entity::File(file) = entity else {
                let name = match entity {
                    Entity::Declaration(id) => &self.declaration(id)?.name,
                    Entity::File(_) => unreachable!(),
                };
                return Err(name_error(format!("'{name}' is not a file namespace")));
            };
            let id = self
                .namespaces
                .get(&file)
                .and_then(|ns| ns.names.get(segment))
                .copied()
                .ok_or_else(|| name_error(format!("unresolved declaration '{segment}'")))?;
            let declaration = self.declaration(id)?;
            if from != file && declaration.visibility != Visibility::Public {
                return Err(name_error(format!("declaration '{segment}' is private")));
            }
            entity = normalize(Entity::Declaration(id))?;
        }
        Ok(entity)
    }

    pub(crate) fn canonical(&self, mut entity: Entity) -> anyhow::Result<Entity> {
        let mut seen = HashSet::new();
        while let Entity::Declaration(id) = entity {
            if !seen.insert(id) {
                return Err(name_error("alias cycle in declaration targets"));
            }
            let declaration = self.declaration(id)?;
            match declaration.alias_target {
                Some(target) => entity = target,
                None if declaration.kind == DeclarationKind::Alias => {
                    return Err(name_error(format!(
                        "unresolved alias '{}'",
                        declaration.name
                    )));
                }
                None => break,
            }
        }
        Ok(entity)
    }

    pub(crate) fn is_type(&self, entity: Entity) -> bool {
        matches!(entity, Entity::Declaration(id) if self.declaration(id).is_ok_and(|d| d.kind == DeclarationKind::Type))
    }
}
