use std::path::Path;
use std::process::{Command, Output};

fn build(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fibc"))
        .current_dir(cwd)
        .args(["build", "--no-std"])
        .args(args)
        .output()
        .unwrap()
}

fn project(root: &Path) {
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/main.fib"), "fn main() int32 { return 42; }").unwrap();
    std::fs::write(
        root.join("build.fib"),
        r#"fn build() int32 {
            build_api::add_source("src/main.fib");
            build_api::set_entry("main");
            build_api::set_output("app");
            return 0;
        }"#,
    )
    .unwrap();
}

fn assert_compiled(output: Output, binary: &Path) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    #[cfg(feature = "llvm")]
    {
        assert!(output.status.success(), "{stderr}");
        assert_eq!(Command::new(binary).status().unwrap().code(), Some(42));
        assert_eq!(
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .last()
                .unwrap(),
            binary.display().to_string()
        );
    }
    #[cfg(not(feature = "llvm"))]
    {
        let _ = binary;
        assert!(!output.status.success());
        assert!(
            stderr.contains("target selection requires the llvm feature"),
            "{stderr}"
        );
    }
}

#[test]
fn discovers_cwd_build_file_and_resolves_project_paths() {
    let directory = tempfile::tempdir().unwrap();
    project(directory.path());
    assert_compiled(build(directory.path(), &[]), &directory.path().join("app"));
}

#[test]
fn missing_default_is_actionable_and_does_not_search_parents() {
    let directory = tempfile::tempdir().unwrap();
    project(directory.path());
    let child = directory.path().join("child");
    std::fs::create_dir(&child).unwrap();
    let output = build(&child, &[]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(&child.join("build.fib").display().to_string()),
        "{stderr}"
    );
    for hint in [
        "create build.fib",
        "--build-file PATH",
        "--single-file PATH",
    ] {
        assert!(stderr.contains(hint), "{stderr}");
    }
}

#[test]
fn explicit_build_file_overrides_default_and_uses_its_own_directory() {
    let directory = tempfile::tempdir().unwrap();
    let selected = directory.path().join("selected");
    project(&selected);
    std::fs::write(directory.path().join("build.fib"), "invalid default").unwrap();
    assert_compiled(
        build(directory.path(), &["--build-file", "selected/build.fib"]),
        &selected.join("app"),
    );
}

#[test]
fn explicit_single_file_overrides_default() {
    let directory = tempfile::tempdir().unwrap();
    project(directory.path());
    std::fs::write(directory.path().join("build.fib"), "invalid default").unwrap();
    assert_compiled(
        build(directory.path(), &["--single-file", "src/main.fib"]),
        &directory.path().join("src/main"),
    );
}

#[test]
fn explicit_options_remain_mutually_exclusive() {
    let directory = tempfile::tempdir().unwrap();
    let output = build(
        directory.path(),
        &[
            "--single-file",
            "missing.fib",
            "--build-file",
            "missing-build.fib",
        ],
    );
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cannot provide single_file and build_file at the same time"),
        "{stderr}"
    );
}
