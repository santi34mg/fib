use std::{fs, process::Command};

#[test]
fn replay_accepts_source_and_reports_diagnostics() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("input.fib");
    for (source, diagnostic) in [
        ("fn main() int32 { return 0; }", "analysis accepted"),
        ("fn (", "parse rejected"),
        ("fn f() int32 { return missing; }", "analysis rejected"),
    ] {
        fs::write(&file, source).unwrap();
        let result = Command::new(env!("CARGO_BIN_EXE_fib-fuzz"))
            .arg("replay")
            .arg(&file)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(String::from_utf8_lossy(&result.stdout).contains("ok"));
        assert!(String::from_utf8_lossy(&result.stdout).contains(diagnostic));
    }
}

#[test]
fn coverage_startup_failure_preserves_input() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("run");
    let result = Command::new(env!("CARGO_BIN_EXE_fib-fuzz"))
        .args(["run", "--evaluations", "1", "--output"])
        .arg(&output)
        .env("FIB_FUZZ_LLVM_PROFDATA", temp.path().join("missing-tool"))
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("coverage preflight"));
    assert!(output.join("input.fib").exists());
}

#[test]
fn non_finding_cannot_be_minimized() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("input.fib");
    fs::write(&file, "fn main() int32 { return 0; }").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_fib-fuzz"))
        .arg("minimize")
        .arg(&file)
        .arg("--output")
        .arg(temp.path().join("small.fib"))
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("does not crash"));
}
