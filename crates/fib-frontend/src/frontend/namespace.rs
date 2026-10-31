use caseless::Caseless;
use std::collections::{HashMap, HashSet};
use unicode_normalization::UnicodeNormalization;

use crate::diagnostics::{CompilerError, ErrorKind, Span};
use crate::plan::{BuildPlan, FileId, FileName, PackageId};

pub use crate::frontend::identifier::{DeclId, Entity, LocalId, ResolvedName};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Visibility {
    Private,
    Directory,
    Package,
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

#[derive(Debug, Clone, Default)]
pub struct ProjectSymbols {
    /// Persistent package identity and normalized module path, computed once.
    pub linkage_modules: HashMap<FileId, (String, String)>,
    pub compiler_declarations: HashMap<DeclId, crate::builtins::BuiltinFunction>,
    pub(crate) package_files: HashMap<(PackageId, String), FileId>,
    pub(crate) dependencies: HashMap<PackageId, HashMap<String, PackageId>>,
    pub(crate) library_prelude: Option<FileId>,

    pub regions: HashMap<FileId, crate::plan::FileRegion>,
    pub files_by_name: HashMap<FileName, FileId>,
    pub namespaces: HashMap<FileId, Namespace>,
}

pub(crate) fn name_error(message: impl Into<String>) -> anyhow::Error {
    CompilerError::new(ErrorKind::Name, message.into()).into()
}

pub(crate) fn normalize_segment(name: &str) -> anyhow::Result<String> {
    let normalized: String = name
        .replace('-', "_")
        .nfc()
        .default_case_fold()
        .nfc()
        .collect();
    if normalized.is_empty()
        || !normalized.chars().enumerate().all(|(index, c)| {
            c == '_'
                || if index == 0 {
                    c.is_alphabetic()
                } else {
                    c.is_alphanumeric() || unicode_normalization::char::is_combining_mark(c)
                }
        })
        || matches!(
            normalized.as_str(),
            "const"
                | "pub"
                | "comp"
                | "if"
                | "else"
                | "while"
                | "break"
                | "continue"
                | "return"
                | "fn"
                | "switch"
                | "when"
                | "struct"
                | "enum"
                | "union"
                | "as"
                | "extern"
                | "callconv"
                | "defer"
        )
    {
        return Err(name_error(format!("invalid namespace segment '{name}'")));
    }
    Ok(normalized)
}

pub(crate) fn normalize_path(path: &str) -> anyhow::Result<String> {
    path.split("::")
        .map(normalize_segment)
        .collect::<anyhow::Result<Vec<_>>>()
        .map(|p| p.join("::"))
}

impl ProjectSymbols {
    /// Look up an already normalized module path within one package.
    pub fn package_file(&self, package: PackageId, module: &str) -> Option<FileId> {
        self.package_files
            .get(&(package, module.to_owned()))
            .copied()
    }

    pub fn from_plan(plan: &BuildPlan) -> anyhow::Result<Self> {
        let mut ids = HashSet::new();
        let mut files_by_name = HashMap::new();
        let regions = plan.file_regions()?;
        let mut package_files = HashMap::new();
        let mut namespace_spellings = HashMap::new();
        let mut dependencies = HashMap::new();
        let mut linkage_modules = HashMap::new();
        for package in &plan.packages {
            let mut names = HashMap::new();
            for (name, target) in &package.dependencies {
                let name = normalize_segment(name)?;
                if name == "root" || names.insert(name, *target).is_some() {
                    return Err(name_error("duplicate or reserved dependency name"));
                }
            }
            dependencies.insert(package.id, names);
        }
        for file in &plan.files {
            if !ids.insert(file.id) {
                return Err(name_error("duplicate file IDs in build plan"));
            }
            if file.name.0.is_empty() || file.name.0.split("::").any(str::is_empty) {
                return Err(name_error(format!("invalid file name '{}'", file.name.0)));
            }
            if plan.packages.is_empty() {
                let name = normalize_path(&file.name.0)?;
                linkage_modules.insert(file.id, ("local:application".into(), name.clone()));
                if files_by_name.insert(FileName(name), file.id).is_some() {
                    return Err(name_error(format!("duplicate file name '{}'", file.name.0)));
                }
            } else {
                let package = regions[&file.id].package;
                let root = &plan
                    .packages
                    .iter()
                    .find(|p| p.id == package)
                    .unwrap()
                    .source_root;
                let relative = std::path::absolute(&file.path)?
                    .strip_prefix(root)?
                    .with_extension("");
                let path = relative
                    .components()
                    .map(|c| {
                        c.as_os_str()
                            .to_str()
                            .map(str::to_owned)
                            .ok_or_else(|| name_error("module paths must be UTF-8"))
                    })
                    .collect::<anyhow::Result<Vec<_>>>()?
                    .join("::");
                let segments: Vec<_> = path.split("::").collect();
                for count in 1..=segments.len() {
                    let spelling = segments[..count].join("::");
                    let key = (package, normalize_path(&spelling)?);
                    if let Some(previous) = namespace_spellings.insert(key, spelling.clone())
                        && previous != spelling
                    {
                        return Err(name_error("normalized namespace spelling collision"));
                    }
                }
                let path = normalize_path(&path)?;
                let identity = &plan
                    .packages
                    .iter()
                    .find(|p| p.id == package)
                    .unwrap()
                    .identity;
                linkage_modules.insert(file.id, (identity.clone(), path.clone()));
                if package_files.insert((package, path), file.id).is_some() {
                    return Err(name_error("duplicate normalized file path"));
                }
                if plan
                    .standard_library
                    .loaded
                    .as_ref()
                    .is_some_and(|std| std.compatibility_package == Some(package))
                    && files_by_name
                        .insert(FileName(normalize_path(&file.name.0)?), file.id)
                        .is_some()
                {
                    return Err(name_error("duplicate compatibility file name"));
                }
            }
        }
        let mut symbols = Self {
            linkage_modules,
            files_by_name,
            namespaces: HashMap::new(),
            regions,
            package_files,
            dependencies,
            library_prelude: plan.standard_library.loaded.as_ref().map(|s| s.prelude),
            ..Self::default()
        };
        let registered: Vec<_> = if plan.packages.is_empty() {
            Vec::new() // flat-plan compatibility retains longest-prefix selection
        } else {
            symbols.package_files.keys().cloned().collect()
        };
        let registered: HashSet<_> = registered.into_iter().collect();
        for (package, path) in &registered {
            let segments: Vec<_> = path.split("::").collect();
            for length in 1..segments.len() {
                if registered.contains(&(*package, segments[..length].join("::"))) {
                    return Err(name_error("file/directory namespace collision"));
                }
            }
        }
        if ids.contains(&PRELUDE_FILE) {
            return Err(name_error("reserved prelude file identity"));
        }
        let mut namespace = Namespace {
            file_id: PRELUDE_FILE,
            declarations: Vec::new(),
            names: HashMap::new(),
        };
        for name in PRELUDE_NAMES {
            let id = DeclId {
                file_id: PRELUDE_FILE,
                index: namespace.declarations.len() as u32,
            };
            namespace.names.insert((*name).into(), id);
            namespace.declarations.push(Declaration {
                name: (*name).into(),
                visibility: Visibility::Public,
                span: Span::new(1, 1),
                kind: DeclarationKind::Type,
                alias_target: None,
            });
        }
        symbols.namespaces.insert(PRELUDE_FILE, namespace);
        Ok(symbols)
    }

    pub(crate) fn prelude_name(&self, name: &str) -> Option<DeclId> {
        self.namespaces.get(&PRELUDE_FILE)?.names.get(name).copied()
    }

    pub(crate) fn is_metatype_syntax(
        &self,
        from: FileId,
        node: &crate::frontend::ast::type_expression::TypeExpression,
    ) -> bool {
        use crate::frontend::{
            ast::type_expression::TypeExpressionKind as K, tokens::builtin::BuiltinType as B,
        };
        match &node.kind {
            K::Builtin(B::MetaType) => true,
            K::QualifiedIdentifier(name) => {
                self.resolve_path_using(from, &name.segments, &mut |e| self.canonical(e)).ok()
                    .is_some_and(|e| matches!(e, Entity::Declaration(id) if Some(id) == self.prelude_name("type")))
                || name.annotation.resolved.is_some_and(|n| matches!(n, ResolvedName::Entity(Entity::Declaration(id)) if Some(id) == self.prelude_name("type")))
            }
            _ => false,
        }
    }

    pub(crate) fn declaration(&self, id: DeclId) -> anyhow::Result<&Declaration> {
        self.namespaces
            .get(&id.file_id)
            .and_then(|ns| ns.declarations.get(id.index as usize))
            .ok_or_else(|| name_error(format!("invalid declaration {id:?}")))
    }

    pub(crate) fn reserved_package_root(&self, from: FileId, name: &str) -> bool {
        let Ok(name) = normalize_segment(name) else {
            return false;
        };
        self.regions.get(&from).is_some_and(|r| {
            self.dependencies
                .get(&r.package)
                .is_some_and(|d| name == "root" || d.contains_key(&name))
        })
    }

    pub(crate) fn reserved_file_root(&self, from: FileId, name: &str) -> bool {
        if self.reserved_package_root(from, name) {
            return true;
        }
        self.files_by_name.iter().any(|(file, id)| {
            file.0.split("::").next() == normalize_segment(name).ok().as_deref()
                && self.regions.get(id).is_none_or(|region| {
                    self.regions
                        .get(&from)
                        .is_some_and(|request| request.package == region.package)
                })
        })
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
        if segments.len() == 1
            && let Some(id) = self.prelude_name(&segments[0])
        {
            return normalize(Entity::Declaration(id));
        }

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
        if let Some((file, count)) = self.file_prefix(from, segments) {
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

    /// Package qualifiers are selected in the requesting file's environment.
    pub(crate) fn file_prefix(&self, from: FileId, segments: &[String]) -> Option<(FileId, usize)> {
        if let Some(region) = self.regions.get(&from)
            && let Some(dependencies) = self.dependencies.get(&region.package)
        {
            let first = normalize_segment(segments.first()?).ok()?;
            let (package, offset) = if first == "root" {
                (region.package, 1)
            } else if let Some(package) = dependencies.get(&first) {
                (*package, 1)
            } else {
                (region.package, 0)
            };
            for count in (offset + 1..=segments.len()).rev() {
                let path = normalize_path(&segments[offset..count].join("::")).ok()?;
                if let Some(file) = self.package_files.get(&(package, path)) {
                    return Some((*file, count));
                }
            }
            if offset != 0 {
                return None;
            }
        }
        (1..=segments.len()).rev().find_map(|count| {
            let path = normalize_path(&segments[..count].join("::")).ok()?;
            self.files_by_name
                .get(&FileName(path))
                .filter(|file| {
                    self.regions.get(file).is_none_or(|region| {
                        self.regions
                            .get(&from)
                            .is_some_and(|request| request.package == region.package)
                    })
                })
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
            if !self.accessible(from, file, declaration.visibility) {
                return Err(name_error(format!("declaration '{segment}' is private")));
            }
            entity = normalize(Entity::Declaration(id))?;
        }
        Ok(entity)
    }

    pub(crate) fn accessible(&self, from: FileId, owner: FileId, visibility: Visibility) -> bool {
        if from == owner || visibility == Visibility::Public {
            return true;
        }
        let (Some(from), Some(owner)) = (self.regions.get(&from), self.regions.get(&owner)) else {
            return false;
        };
        from.package == owner.package
            && match visibility {
                Visibility::Package => true,
                Visibility::Directory => from.directory.starts_with(&owner.directory),
                Visibility::Private | Visibility::Public => false,
            }
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

/// Internal identity only; no public qualified compiler namespace is introduced.
pub(crate) const PRELUDE_FILE: FileId = FileId(u32::MAX);
pub(crate) const PRELUDE_NAMES: &[&str] = &[
    "type", "void", "int", "uint", "int8", "int16", "int32", "int64", "int128", "uint8", "uint16",
    "uint32", "uint64", "uint128", "float16", "float32", "float64", "float128", "bool", "never",
    "usize",
];

#[cfg(test)]
mod normalization_tests {
    use super::normalize_segment;

    #[test]
    fn full_unicode_folding_preserves_canonical_equivalence_and_combining_marks() {
        for (first, second) in [
            ("Straße", "STRASSE"),
            ("Cafe\u{301}", "CAFÉ"),
            ("Σ", "ς"),
            ("A-B", "a_b"),
        ] {
            assert_eq!(
                normalize_segment(first).unwrap(),
                normalize_segment(second).unwrap()
            );
        }
        assert_eq!(normalize_segment("İ").unwrap(), "i\u{307}");
        assert_ne!(
            normalize_segment("İ").unwrap(),
            normalize_segment("i").unwrap()
        );
        for invalid in ["", "bad name", "ſtruct", "\u{301}start"] {
            assert!(normalize_segment(invalid).is_err(), "{invalid}");
        }
    }
}
