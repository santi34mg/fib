//! Installed, version-matched standard-library sources. Loading precedes parsing.
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use crate::plan::{BuildPlan, FileId, FileName, PackageId, PackageSpec, SourceFile};
use anyhow::Context;

pub const DECLARATION_REVISION: u32 = 6;

#[derive(Debug, Clone)]
pub struct StandardLibrary {
    pub enabled: bool,
    pub sysroot: Option<PathBuf>,
    /// Operating-system family selecting per-target std sources (`linux`,
    /// `darwin`). None uses the host the compiler was built for.
    pub target_os: Option<String>,
    pub(crate) loaded: Option<Registration>,
}

/// The std target family of the compiler's host.
pub fn host_target_os() -> &'static str {
    match std::env::consts::OS {
        "macos" | "ios" => "darwin",
        other => other,
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Registration {
    pub package: PackageId,
    pub compatibility_package: Option<PackageId>,
    pub memory: FileId,
    pub os: FileId,
    pub private: FileId,
    pub prelude: FileId,
}

impl Default for StandardLibrary {
    fn default() -> Self {
        Self {
            enabled: true,
            sysroot: None,
            target_os: None,
            loaded: None,
        }
    }
}

impl StandardLibrary {
    /// Identity of the installed source package after plan preparation.
    pub fn package(&self) -> Option<PackageId> {
        self.loaded
            .as_ref()
            .map(|registration| registration.package)
    }

    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Self::default()
        }
    }
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self {
            sysroot: Some(path.into()),
            ..Self::default()
        }
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    compiler_version: String,
    declaration_revision: u32,
    sources: Vec<String>,
    /// Per-target-OS adjustments keyed by family (`linux`, `darwin`).
    #[serde(default)]
    targets: BTreeMap<String, TargetSources>,
}

/// Target-specific source selection. Logical names (and so module paths such
/// as `std::os`) stay those of `sources`; only the backing file changes.
#[derive(serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct TargetSources {
    /// Logical source -> file implementing it for this target.
    #[serde(default)]
    replace: BTreeMap<String, String>,
    /// Logical sources unavailable on this target.
    #[serde(default)]
    exclude: Vec<String>,
}

pub fn installed_sysroot(explicit: Option<&Path>) -> anyhow::Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path.to_owned());
    }
    if let Some(path) = std::env::var_os("FIB_SYSROOT") {
        return Ok(path.into());
    }
    let executable = std::env::current_exe()?;
    let prefix = executable
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| anyhow::anyhow!("cannot determine compiler installation prefix"))?;
    Ok(prefix.join("lib/fib").join(env!("CARGO_PKG_VERSION")))
}

/// Find the source tree owning an editor document, without discovering modules.
/// Its manifest still defines membership and is validated during preparation.
pub fn source_sysroot(source: &Path) -> Option<PathBuf> {
    source.parent()?.ancestors().find_map(|directory| {
        if directory.file_name()? != "std" {
            return None;
        }
        let sysroot = directory.parent()?;
        sysroot
            .join("sysroot.toml")
            .is_file()
            .then(|| sysroot.to_owned())
    })
}

impl BuildPlan {
    /// Return a complete plan; never mutate the caller or discover during lookup.
    pub fn prepare(&self) -> anyhow::Result<Self> {
        if !self.standard_library.enabled || self.standard_library.loaded.is_some() {
            return Ok(self.clone());
        }
        let source_context = if self.packages.is_empty() && self.files.len() == 1 {
            source_sysroot(&self.files[0].path)
        } else {
            None
        };
        let editing_std = source_context.is_some();
        let selected_source = if editing_std {
            Some((self.files[0].id, self.files[0].path.canonicalize()?))
        } else {
            None
        };
        let sysroot = match source_context {
            Some(sysroot) => sysroot,
            None => installed_sysroot(self.standard_library.sysroot.as_deref())?,
        };
        let manifest_path = sysroot.join("sysroot.toml");
        let manifest: Manifest = toml::from_str(&std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("reading standard-library sysroot {}; install matching sources or use --sysroot/--no-std", manifest_path.display()))?)?;
        anyhow::ensure!(
            manifest.compiler_version == env!("CARGO_PKG_VERSION")
                && manifest.declaration_revision == DECLARATION_REVISION,
            "incompatible standard-library sysroot: compiler version or declaration revision mismatch"
        );
        let root = sysroot
            .join("std")
            .canonicalize()
            .context("resolving std source root")?;
        let mut plan = self.clone();
        // The flat-plan adapter becomes one explicit application package without
        // changing its registered logical file names.
        let compatibility_package =
            (plan.packages.is_empty() && !editing_std).then_some(PackageId(0));
        if plan.packages.is_empty() && !editing_std {
            let mut root = plan
                .files
                .first()
                .ok_or_else(|| anyhow::anyhow!("empty source plan"))?
                .path
                .parent()
                .unwrap_or(Path::new("."))
                .to_owned();
            root = std::path::absolute(root)?;
            while !plan
                .files
                .iter()
                .all(|f| std::path::absolute(&f.path).is_ok_and(|p| p.starts_with(&root)))
            {
                anyhow::ensure!(root.pop(), "sources have no common source root");
            }
            plan.packages.push(PackageSpec {
                id: PackageId(0),
                identity: "local:application".into(),
                source_root: root,
                dependencies: BTreeMap::new(),
                files: plan.files.iter().map(|f| f.id).collect(),
            });
        }
        let package = PackageId(
            plan.packages
                .iter()
                .map(|p| p.id.0)
                .max()
                .unwrap_or(0)
                .checked_add(1)
                .ok_or_else(|| anyhow::anyhow!("package ID space exhausted"))?,
        );
        for pkg in &mut plan.packages {
            anyhow::ensure!(
                !pkg.dependencies
                    .keys()
                    .any(|name| name.eq_ignore_ascii_case("std")),
                "dependency name 'std' is already registered"
            );
            pkg.dependencies.insert("std".into(), package);
        }
        let mut next = plan.files.iter().map(|f| f.id.0).max().unwrap_or(0);
        let mut files = BTreeMap::new();
        let mut selected = false;
        let valid = |relative: &str| {
            let path = Path::new(relative);
            anyhow::ensure!(
                path.extension().is_some_and(|s| s == "fib")
                    && path
                        .components()
                        .all(|c| matches!(c, std::path::Component::Normal(_))),
                "invalid std source manifest path '{relative}'"
            );
            Ok(())
        };
        let mut target_os = self
            .standard_library
            .target_os
            .clone()
            .unwrap_or_else(|| host_target_os().to_owned());
        // An opened std file selects a target that loads it, preferring the
        // host, so editing darwin/os.fib or the Linux os.fib works anywhere.
        if let Some((_, selected_path)) = &selected_source {
            let loads = |family: &str| {
                let target = manifest.targets.get(family);
                manifest.sources.iter().any(|logical| {
                    if target.is_some_and(|t| t.exclude.contains(logical)) {
                        return false;
                    }
                    let file = target
                        .and_then(|t| t.replace.get(logical))
                        .unwrap_or(logical);
                    root.join(file)
                        .canonicalize()
                        .is_ok_and(|p| &p == selected_path)
                })
            };
            if !loads(&target_os)
                && let Some(family) = manifest.targets.keys().find(|family| loads(family))
            {
                target_os = family.clone();
            }
        }
        let mut targets = manifest.targets;
        let target = targets.remove(&target_os).unwrap_or_default();
        for (logical, file) in &target.replace {
            valid(logical)?;
            valid(file)?;
            anyhow::ensure!(
                manifest.sources.contains(logical),
                "std target '{target_os}' replaces unlisted source '{logical}'"
            );
        }
        for logical in &target.exclude {
            anyhow::ensure!(
                manifest.sources.contains(logical),
                "std target '{target_os}' excludes unlisted source '{logical}'"
            );
        }
        let mut paths = manifest.sources;
        paths.retain(|logical| !target.exclude.contains(logical));
        paths.sort();
        for relative in paths {
            let relative_path = Path::new(&relative);
            valid(&relative)?;
            let file = target
                .replace
                .get(&relative)
                .map_or(relative_path, Path::new);
            let path = root.join(file).canonicalize()?;
            anyhow::ensure!(
                path.starts_with(&root) && path.is_file(),
                "std source escapes its source root"
            );
            let name = relative_path
                .with_extension("")
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("::");
            let id = if let Some((id, selected_path)) = &selected_source
                && selected_path == &path
            {
                // Reuse the open document instead of also compiling an
                // application copy whose `root` would be a different package.
                selected = true;
                *id
            } else {
                next = next
                    .checked_add(1)
                    .filter(|id| *id != u32::MAX)
                    .ok_or_else(|| anyhow::anyhow!("file ID space exhausted"))?;
                let id = FileId(next);
                plan.files.push(SourceFile {
                    id,
                    name: FileName(name.clone()),
                    path: path.clone(),
                });
                id
            };
            anyhow::ensure!(
                files.insert(name.clone(), id).is_none(),
                "duplicate std source '{name}'"
            );
        }
        anyhow::ensure!(
            !editing_std || selected,
            "source is not listed in the std sysroot manifest"
        );
        let required = |name: &str| {
            files
                .get(name)
                .copied()
                .ok_or_else(|| anyhow::anyhow!("std manifest is missing module '{name}'"))
        };
        let registration = Registration {
            package,
            compatibility_package,
            memory: required("memory")?,
            os: required("os")?,
            private: required("os_abi")?,
            prelude: required("prelude")?,
        };
        required("result")?;
        required("map")?;
        required("process")?;
        plan.packages.push(PackageSpec {
            id: package,
            identity: format!(
                "fib:std@{}:decl{}",
                manifest.compiler_version, manifest.declaration_revision
            ),
            source_root: root,
            dependencies: BTreeMap::new(),
            files: files.values().copied().collect(),
        });
        plan.standard_library.sysroot = Some(sysroot);
        plan.standard_library.loaded = Some(registration);
        crate::frontend::namespace::ProjectSymbols::from_plan(&plan)?;
        Ok(plan)
    }
}
