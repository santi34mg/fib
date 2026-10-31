//! Build and smoke-test a relocatable Linux x86-64 release bundle.
use std::{env, fs, path::PathBuf, process::Command};

use anyhow::{Context, Result, ensure};
use clap::Parser;

#[derive(Parser)]
struct Options {
    /// Directory for the archive and SHA-256 checksum.
    #[arg(long, default_value = "dist")]
    output: PathBuf,
    /// Require the release tag to match the compiled workspace version.
    #[arg(long)]
    tag: Option<String>,
}

fn run(command: &mut Command) -> Result<()> {
    let status = command
        .status()
        .with_context(|| format!("starting {command:?}"))?;
    ensure!(status.success(), "command failed: {command:?}");
    Ok(())
}

fn main() -> Result<()> {
    let options = Options::parse();
    ensure!(
        cfg!(all(target_os = "linux", target_arch = "x86_64")),
        "release bundles currently require Linux x86-64"
    );
    let version = env!("CARGO_PKG_VERSION");
    let tag = options
        .tag
        .or_else(|| env::var("FIB_RELEASE_TAG").ok().filter(|s| !s.is_empty()));
    if let Some(tag) = tag {
        ensure!(
            tag == format!("v{version}"),
            "release tag {tag} does not match v{version}"
        );
    }
    let output = std::path::absolute(options.output)?;
    fs::create_dir_all(&output)?;
    let name = format!("fib-{version}-linux-x86_64");
    let archive_name = format!("{name}.tar.gz");
    let checksum_name = format!("{archive_name}.sha256");
    ensure!(
        !output.join(&archive_name).exists() && !output.join(&checksum_name).exists(),
        "release output already exists; choose a fresh output directory"
    );
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let temporary = tempfile::tempdir()?;
    let prefix = temporary.path().join(&name);
    run(
        Command::new(env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .current_dir(&repository)
            .args([
                "run",
                "--locked",
                "--no-default-features",
                "--bin",
                "fib-install",
                "--",
                "--prefix",
            ])
            .arg(&prefix),
    )?;
    fs::copy(repository.join("LICENSE"), prefix.join("LICENSE"))?;
    fs::copy(repository.join("CHANGELOG.md"), prefix.join("CHANGELOG.md"))?;
    fs::write(
        prefix.join("README.txt"),
        format!(
            "Fib {version} for Linux x86-64\n\n\
         Keep bin/ and lib/ together when extracting or moving this directory.\n\
         Add its bin/ directory to PATH; fibc finds matching std automatically.\n\
         Native builds require host cc and the runtime libraries in runtime-dependencies.txt.\n\
         Official bundles are built on Ubuntu 24.04; older glibc systems are not guaranteed.\n\
         LLVM development libraries and Rust are not required to use a compatible bundle.\n\
         The full compiler source is in the GitHub release source archives.\n"
        ),
    )?;
    let dependencies = Command::new("ldd").arg(prefix.join("bin/fibc")).output()?;
    ensure!(
        dependencies.status.success(),
        "cannot inspect runtime dependencies"
    );
    let listing = String::from_utf8(dependencies.stdout)?;
    ensure!(
        !listing.contains("not found"),
        "compiler has missing runtime libraries: {listing}"
    );
    fs::write(prefix.join("runtime-dependencies.txt"), listing)?;

    // Test the actual archive after extraction, outside the source checkout.
    let staged_archive = temporary.path().join(&archive_name);
    run(Command::new("tar")
        .args(["-czf"])
        .arg(&staged_archive)
        .arg("-C")
        .arg(temporary.path())
        .arg(&name))?;
    let unpack = tempfile::tempdir()?;
    run(Command::new("tar")
        .arg("-xzf")
        .arg(&staged_archive)
        .arg("-C")
        .arg(unpack.path()))?;
    let installed = unpack.path().join(&name);
    let source = unpack.path().join("hello_world.fib");
    fs::copy(repository.join("samples/hello_world.fib"), &source)?;
    run(Command::new(installed.join("bin/fibc"))
        .current_dir(unpack.path())
        .env_remove("FIB_SYSROOT")
        .args(["build", "--single-file"])
        .arg(&source))?;
    let hello = Command::new(unpack.path().join("hello_world")).output()?;
    ensure!(
        hello.status.success() && hello.stdout == b"Hello, World!\n" && hello.stderr.is_empty(),
        "extracted compiler/std smoke test failed"
    );
    run(Command::new(installed.join("bin/fib-lsp")).arg("--help"))?;

    let checksum = Command::new("sha256sum")
        .current_dir(temporary.path())
        .arg(&archive_name)
        .output()?;
    ensure!(checksum.status.success(), "checksum generation failed");
    // Publish local outputs only once archive construction and smoke tests pass.
    fs::copy(&staged_archive, output.join(&archive_name))?;
    fs::write(output.join(&checksum_name), checksum.stdout)?;
    println!(
        "Created {} and {}",
        output.join(archive_name).display(),
        output.join(checksum_name).display()
    );
    Ok(())
}
