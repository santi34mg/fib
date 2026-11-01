//! Every top-level sample must compile and execute with its documented behavior.
#![cfg(feature = "llvm")]

use std::{
    fs,
    process::Command,
    time::{Duration, Instant},
};

fn expected(name: &str) -> (i32, String) {
    let (code, output) = match name {
        "allocator"
        | "builtin_scalars"
        | "const_bindings"
        | "deferred_blocks"
        | "enum_payloads"
        | "enum_runtime"
        | "generic_map"
        | "indexed_globals"
        | "memory"
        | "process_entry"
        | "scalar_loops"
        | "sizeof"
        | "specification_runtime"
        | "struct_storage" => (42, ""),
        "function_pointer_callback" => (20, ""),
        "minimal" => (0, ""),
        "comp_members" => (0, "woofwoofwoof"),
        "bitwise_ops" => (
            0,
            "Packed IPv4: 3232235876\nOctet 0: 192\nOctet 1: 168\nOctet 2: 1\nOctet 3: 100\n\nInitial permissions: 3\nAfter grant EXEC:    7\nAfter revoke WRITE:  5\n  [x] READ\n  [x] EXEC\nBits set in perms: 2\n",
        ),
        "enums" => (0, "green\n"),
        "formatting" => (
            0,
            "Hello Alice, score: -42, max: 18446744073709551615, active: true {ok}\n",
        ),
        "hello_world" => (0, "Hello, World!\n"),
        "linked_list" => (0, "List:   10 -> 20 -> 30 -> null\nLength: 3\n"),
        "memory_pool" => (
            0,
            "Pool: block_size=4, capacity=6\nBlock 0: wrote 7\nBlock 1: wrote 14\nBlock 2: wrote 21\nBlock 3: wrote 28\nBlock 4: wrote 35\nBlock 5: wrote 42\nAlloc 6 failed: pool exhausted\nUsed: 6 / 6\n",
        ),
        "multiple_returns" => (0, "q=3 r=2\n"),
        "nested_assign" => (0, "got 42\n"),
        "slices" => (0, "len=4 sum=100 s0=10\nafter s1=99 arr1=99\n"),
        "sorting" => (0, "[ 1  2  3  5  6  7  8  10 ]\n"),
        "string_functions" => (0, "Hello, fib!"),
        "switch_enum" => (0, "red\ngreen\nblue\n"),
        "syscalls" => (0, "Hello from std!\n"),
        "tagged_union" => (0, "int=7\nbool=1\neof\n"),
        "unsigned_ops" => (
            0,
            "div32: 4000000000 / 7 = 571428571\nrem32: 4000000000 % 7 = 3\ndiv64: 10000000000000000000 / 3 = 3333333333333333333\nrem64: 10000000000000000000 % 3 = 1\ncmp32: 4000000000 > 7 = 1\ncmp32: 7 < 4000000000 = 1\ncmp32: 4000000000 >= 4000000000 = 1\ncmp32: 7 <= 7 = 1\nshr32: 4000000000 >> 1 = 2000000000\nshr64: 10000000000000000000 >> 1 = 5000000000000000000\n",
        ),
        "fib_bench" => {
            let mut output = "Fibonacci benchmark (recursive, no memoization)\n-----------------------------------------------\n".to_owned();
            let (mut a, mut b) = (0_u64, 1_u64);
            for i in 0..=20 {
                output.push_str(&format!("fib({i}) = {a}\n"));
                (a, b) = (b, a + b);
            }
            return (0, output);
        }
        _ => panic!("add an execution expectation for sample {name}"),
    };
    (code, output.to_owned())
}

#[test]
fn all_samples_compile_and_execute() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut sources: Vec<_> = fs::read_dir(root.join("samples"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "fib"))
        .collect();
    sources.sort();
    assert!(!sources.is_empty());
    for source in sources {
        let name = source.file_stem().unwrap().to_str().unwrap();
        let (code, stdout) = expected(name);
        let directory = tempfile::tempdir().unwrap();
        // Preserve filenames: a reserved namespace filename must fail this sweep.
        let copy = directory.path().join(source.file_name().unwrap());
        fs::copy(&source, &copy).unwrap();
        let build = Command::new(env!("CARGO_BIN_EXE_fibc"))
            .args(["build", "--sysroot"])
            .arg(root.join("library"))
            .arg("--single-file")
            .arg(&copy)
            .output()
            .unwrap();
        assert!(
            build.status.success(),
            "{name}: {}{}",
            String::from_utf8_lossy(&build.stdout),
            String::from_utf8_lossy(&build.stderr)
        );
        let out_path = directory.path().join("stdout");
        let err_path = directory.path().join("stderr");
        let mut child = Command::new(copy.with_extension(""))
            .env("FIB_EXIT", "42")
            .stdout(fs::File::create(&out_path).unwrap())
            .stderr(fs::File::create(&err_path).unwrap())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("{name}: execution exceeded ten seconds");
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(status.code(), Some(code), "{name}");
        assert_eq!(fs::read_to_string(out_path).unwrap(), stdout, "{name}");
        assert_eq!(fs::read_to_string(err_path).unwrap(), "", "{name}");
    }
}
