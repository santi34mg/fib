//! End-to-end regression net: compile every `samples/*.fib` with the library
//! API and execute the resulting binary.
//!
//! Samples with fast, deterministic output pin `stdout` exactly, turning
//! `samples/` into an executable spec. `sorting` pins its single output
//! line; `fib_bench` is compile-only (naive `fib(50)` takes hours to run).
//! Each case builds into its own temp dir so parallel test threads never
//! share output paths.
//!
//! Requires the `llvm` feature plus a C compiler (`--cc`/`$CC`, else
//! `clang-17`/`clang`); the file is empty without `llvm`.

#![cfg(feature = "llvm")]

use std::path::{Path, PathBuf};
use std::process::Command;

use fibc::{CompilationOptions, CompileOutput};

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Compile `samples/<name>.fib` into `out_dir`, returning the binary path.
fn compile_sample(name: &str, out_dir: &Path) -> PathBuf {
    let root = project_root();
    let mut opts = CompilationOptions::new(root.join("samples").join(format!("{}.fib", name)));
    opts.include_paths = vec![root.join("std")];
    opts.output = Some(out_dir.join(name));
    match fibc::compile_project(&opts).unwrap_or_else(|e| panic!("e2e compile {}: {}", name, e)) {
        CompileOutput::Binary { binary, .. } => binary,
        other => panic!("e2e compile {}: expected Binary, got {:?}", name, other),
    }
}

/// Run `binary`, returning `(exit_code, stdout)`.
fn run_binary(binary: &Path) -> (i32, String) {
    let out = Command::new(binary)
        .output()
        .unwrap_or_else(|e| panic!("e2e run {}: {}", binary.display(), e));
    let stdout = String::from_utf8(out.stdout).expect("binary stdout is UTF-8");
    (out.status.code().unwrap_or(-1), stdout)
}

/// Compile `samples/<name>.fib` in a fresh temp dir, run it, return output.
fn compile_and_run(name: &str) -> (i32, String) {
    let dir = tempfile::tempdir().expect("e2e tempdir");
    let binary = compile_sample(name, dir.path());
    run_binary(&binary)
}

/// Assert `samples/<name>.fib` exits 0 with exactly `expected` on stdout.
fn check_sample(name: &str, expected: &str) {
    let (code, stdout) = compile_and_run(name);
    assert_eq!(code, 0, "e2e {}: exit code", name);
    assert_eq!(stdout, expected, "e2e {}: stdout", name);
}

#[test]
fn e2e_hello_world() {
    check_sample("hello_world", "Hello, World!\n");
}

#[test]
fn e2e_minimal() {
    check_sample("minimal", "");
}

#[test]
fn e2e_enums() {
    check_sample("enums", "color discriminant = 1\n");
}

#[test]
fn e2e_switch_enum() {
    check_sample("switch_enum", "red\ngreen\nblue\n");
}

#[test]
fn e2e_bitwise_ops() {
    check_sample(
        "bitwise_ops",
        concat!(
            "Packed IPv4: 0xC0A80164\n",
            "Octet 0: 192\n",
            "Octet 1: 168\n",
            "Octet 2: 1\n",
            "Octet 3: 100\n",
            "\n",
            "Initial permissions: 0x3\n",
            "After grant EXEC:    0x7\n",
            "After revoke WRITE:  0x5\n",
            "  [x] READ\n",
            "  [x] EXEC\n",
            "Bits set in perms: 2\n",
        ),
    );
}

#[test]
fn e2e_unsigned_ops() {
    check_sample(
        "unsigned_ops",
        concat!(
            "div32: 4000000000 / 7 = 571428571\n",
            "rem32: 4000000000 % 7 = 3\n",
            "div64: 10000000000000000000 / 3 = 3333333333333333333\n",
            "rem64: 10000000000000000000 % 3 = 1\n",
            "cmp32: 4000000000 > 7 = 1\n",
            "cmp32: 7 < 4000000000 = 1\n",
            "cmp32: 4000000000 >= 4000000000 = 1\n",
            "cmp32: 7 <= 7 = 1\n",
            "shr32: 4000000000 >> 1 = 2000000000\n",
            "shr64: 10000000000000000000 >> 1 = 5000000000000000000\n",
        ),
    );
}

#[test]
fn e2e_linked_list() {
    check_sample("linked_list", "List:   10 -> 20 -> 30 -> null\nLength: 3\n");
}

#[test]
fn e2e_memory_pool() {
    check_sample(
        "memory_pool",
        concat!(
            "Pool: block_size=4, capacity=6\n",
            "Block 0: wrote 7\n",
            "Block 1: wrote 14\n",
            "Block 2: wrote 21\n",
            "Block 3: wrote 28\n",
            "Block 4: wrote 35\n",
            "Block 5: wrote 42\n",
            "Alloc 6 failed: pool exhausted\n",
            "Used: 6 / 6\n",
        ),
    );
}

#[test]
fn e2e_multiple_returns() {
    check_sample("multiple_returns", "q=3 r=2\n");
}

#[test]
fn e2e_nested_assign() {
    check_sample("nested_assign", "got 42\n");
}

#[test]
fn e2e_sorting() {
    check_sample("sorting", "[ 1  2  3  5  6  7  8  10 ]\n");
}

#[test]
fn e2e_slices() {
    check_sample(
        "slices",
        concat!("len=4 sum=100 s0=10\n", "after s1=99 arr1=99\n",),
    );
}

#[test]
fn e2e_string_builtins() {
    check_sample("string_builtins", "Hello, fib! (len=11, eq=1)\n");
}

#[test]
fn e2e_tagged_union() {
    check_sample("tagged_union", "int=7\nbool=1\neof\n");
}

#[test]
fn e2e_fib_bench_compiles() {
    // Compile-only: naive fib(50) takes hours to execute.
    let dir = tempfile::tempdir().expect("e2e tempdir");
    let binary = compile_sample("fib_bench", dir.path());
    assert!(binary.is_file(), "expected binary at {}", binary.display());
}

#[test]
fn e2e_broken_source_is_an_error_not_a_panic() {
    let dir = tempfile::tempdir().expect("e2e tempdir");
    let src_path = dir.path().join("broken.fib");
    std::fs::write(&src_path, "fn broken( @int { return 0 }").expect("write broken.fib");
    let mut opts = CompilationOptions::new(src_path);
    opts.output = Some(dir.path().join("broken"));
    let err = fibc::compile_project(&opts).expect_err("broken source must fail");
    // Structured ParseError: file path + line info, not a generic string.
    let msg = err.to_string();
    assert!(msg.contains("broken.fib"), "error names file: {}", msg);
}
