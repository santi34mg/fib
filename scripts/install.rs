//! Install compiler binaries and their matching readable standard-library sources.
//! Run with `cargo run --no-default-features --bin fib-install -- [options]`.
use std::{
    collections::BTreeMap,
    env, fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{Context, Result, bail, ensure};
use clap::Parser;

#[derive(Parser)]
#[command(
    name = "fib-install",
    about = "Install fibc, fib-lsp and matching std sources"
)]
struct Options {
    /// Installation prefix (defaults to $HOME/.local).
    #[arg(long, value_name = "DIRECTORY")]
    prefix: Option<PathBuf>,
    /// Build debug binaries instead of release binaries.
    #[arg(long)]
    debug: bool,
}

fn main() -> Result<()> {
    let options = Options::parse();
    let prefix = options
        .prefix
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local")))
        .context("HOME is not set; provide --prefix DIRECTORY")?;
    let prefix = std::path::absolute(prefix)?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
    let artifacts = build(repository, options.debug)?;
    let destination = prefix.join("lib/fib").join(env!("CARGO_PKG_VERSION"));

    for name in ["fibc", "fib-lsp"] {
        let artifact = artifacts
            .get(name)
            .with_context(|| format!("Cargo did not produce {name}"))?;
        let installed_name = artifact
            .file_name()
            .context("binary artifact has no file name")?;
        install_file(artifact, &prefix.join("bin").join(installed_name), true)?;
    }
    let source_root = repository.join("library/std");
    for entry in walkdir::WalkDir::new(&source_root).sort_by_file_name() {
        let entry = entry.context("reading std source tree")?;
        let target = destination
            .join("std")
            .join(entry.path().strip_prefix(&source_root)?);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target)?;
        } else {
            ensure!(
                entry.file_type().is_file(),
                "unsupported std source entry: {}",
                entry.path().display()
            );
            install_file(entry.path(), &target, false)?;
        }
    }
    install_file(
        &repository.join("library/sysroot.toml"),
        &destination.join("sysroot.toml"),
        false,
    )?;
    println!(
        "Installed fibc, fib-lsp and std {} under {}",
        env!("CARGO_PKG_VERSION"),
        prefix.display()
    );
    Ok(())
}

fn build(repository: &Path, debug: bool) -> Result<BTreeMap<String, PathBuf>> {
    println!(
        "Building {} compiler binaries...",
        if debug { "debug" } else { "release" }
    );
    let mut command = Command::new(env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    command.current_dir(repository).args([
        "build",
        "--bin",
        "fibc",
        "--bin",
        "fib-lsp",
        "--message-format=json-render-diagnostics",
    ]);
    if !debug {
        command.arg("--release");
    }
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .context("starting Cargo build")?;
    let output = child.stdout.take().context("Cargo stdout is unavailable")?;
    let artifacts = (|| -> Result<BTreeMap<String, PathBuf>> {
        let mut artifacts = BTreeMap::new();
        for line in BufReader::new(output).lines() {
            let message: serde_json::Value = serde_json::from_str(&line?)?;
            if message["reason"] != "compiler-artifact" {
                continue;
            }
            if let (Some(name @ ("fibc" | "fib-lsp")), Some(executable)) = (
                message["target"]["name"].as_str(),
                message["executable"].as_str(),
            ) {
                artifacts.insert(name.to_owned(), PathBuf::from(executable));
            }
        }
        Ok(artifacts)
    })();
    if !child.wait().context("waiting for Cargo build")?.success() {
        bail!("compiler build failed; installation was not started");
    }
    artifacts.context("reading Cargo artifact paths")
}

fn install_file(source: &Path, destination: &Path, executable: bool) -> Result<()> {
    let directory = destination
        .parent()
        .context("installation destination has no parent")?;
    fs::create_dir_all(directory)?;
    let temporary = tempfile::NamedTempFile::new_in(directory)?;
    fs::copy(source, temporary.path()).with_context(|| format!("copying {}", source.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            temporary.path(),
            fs::Permissions::from_mode(if executable { 0o755 } else { 0o644 }),
        )?;
    }
    #[cfg(not(unix))]
    let _ = executable;
    temporary
        .persist(destination)
        .with_context(|| format!("installing {}", destination.display()))?;
    Ok(())
}
