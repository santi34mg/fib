//! Requires LLVM 21 and a host C linker. No skipped tests when LLVM is enabled.
#![cfg(feature = "llvm")]
use fibc::{
    driver::{PipelineGoal, compiler_output::CompilationOutput, run_pipeline},
    plan::BuildPlan,
};
use std::process::Command;

#[test]
fn single_file_cli_compiles_links_and_runs() {
    for status in [0, 42, 127] {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("main.fib");
        let text = if status == 42 {
            include_str!("programs/return_42.fib").to_owned()
        } else {
            format!("fn main() int32 {{ return {status}; }}")
        };
        std::fs::write(&source, text).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_fibc"))
            .args(["build", "--single-file"])
            .arg(&source)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let binary = source.with_extension(if cfg!(windows) { "exe" } else { "" });
        assert_eq!(Command::new(binary).status().unwrap().code(), Some(status));
    }
}

#[test]
fn object_goal_emits_linkable_object_and_source_is_protected() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("main.fib");
    std::fs::write(&source, include_str!("programs/return_42.fib")).unwrap();
    let mut plan = BuildPlan::single_file(&source).unwrap();
    let CompilationOutput::Objects(objects) = run_pipeline(&plan, PipelineGoal::Objects).unwrap()
    else {
        panic!("expected objects")
    };
    assert_eq!(objects.len(), 1);
    assert!(std::fs::metadata(&objects[0]).unwrap().len() > 0);
    let binary = directory.path().join("linked");
    assert!(
        Command::new("cc")
            .arg(&objects[0])
            .arg("-o")
            .arg(&binary)
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(Command::new(binary).status().unwrap().code(), Some(42));
    plan.output = source.clone();
    assert!(
        run_pipeline(&plan, PipelineGoal::Binary)
            .unwrap_err()
            .to_string()
            .contains("overwrite source")
    );
    assert_eq!(
        std::fs::read_to_string(source).unwrap(),
        include_str!("programs/return_42.fib")
    );
}

#[test]
fn unsupported_source_exits_unsuccessfully_without_binary() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("main.fib");
    std::fs::write(&source, "fn main() int32 { return 20 + 22; }").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_fibc"))
        .args(["build", "--single-file"])
        .arg(&source)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unsupported binary operator"));
    assert!(!source.with_extension("").exists());
}

#[test]
fn linker_failure_preserves_existing_output() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("main.fib");
    std::fs::write(&source, include_str!("programs/return_42.fib")).unwrap();
    let plan = BuildPlan::single_file(&source).unwrap();
    std::fs::write(&plan.output, b"existing output").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_fibc"))
        .args(["build", "--single-file"])
        .arg(source)
        .env("PATH", "")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("host linker cc"));
    assert_eq!(std::fs::read(&plan.output).unwrap(), b"existing output");
}

#[test]
fn conditionals_helpers_and_joins_execute() {
    let cases = [
        (include_str!("programs/conditionals.fib").to_owned(), 42),
        (include_str!("programs/conditionals.fib").replace("choose(5)", "choose(17)"), 17),
        ("fn main() int32 { x := 0; if true { x = 42; }; return x; }".into(), 42),
        ("fn f(x: int32) int32 { answer := 1; if (x < 10) { if (x == 5) { answer = 42; } else { answer = 3; }; } else { answer = 7; }; return answer; } fn main() int32 { return f(5); }".into(), 42),
        ("fn main() int32 { return f(7); } fn f(x: int32) int32 { if (x == 7) { return 42; }; return 3; }".into(), 42),
        ("fn f(x: int32) int32 { if (x < 0) { return 42; } else { return 7; }; } fn main() int32 { return f(-1); }".into(), 42),
        ("fn f(x: int32) int32 { flag := x == 7; if (x > 0) { flag = false; }; if (flag == false) { return 42; } else { return 7; }; } fn main() int32 { return f(7); }".into(), 42),
    ];
    let extra = [
        ("fn f(x: int32) int32 { a := 1; b := 2; if (x < 10) { a = 42; b = 7; } else { a = 7; b = 42; }; if (a > b) { return a; } else { return b; }; } fn main() int32 { return f(5); }".to_owned(), 42),
        ("fn f(x: int32) int32 { a := 1; b := 2; if (x < 10) { a = 42; b = 7; } else { a = 7; b = 42; }; if (a > b) { return a; } else { return b; }; } fn main() int32 { return f(17); }".to_owned(), 42),
        ("fn f(x: int32) int32 { y := 0; if (x == 1) { y = 42; } else { return 7; }; return y; } fn main() int32 { return f(1); }".to_owned(), 42),
        ("fn main() int32 { x := -2147483648; if (x < 0) { return 42; }; return 7; }".to_owned(), 42),
    ];
    for (source_text, expected) in cases.into_iter().chain(extra) {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("main.fib");
        std::fs::write(&source, &source_text).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_fibc"))
            .args(["build", "--single-file"])
            .arg(&source)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{source_text}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            Command::new(source.with_extension(if cfg!(windows) { "exe" } else { "" }))
                .status()
                .unwrap()
                .code(),
            Some(expected),
            "{source_text}"
        );
    }
}
