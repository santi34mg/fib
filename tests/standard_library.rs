//! Installed source modules, compiler identities and native syscall acceptance.
use fibc::{
    driver::parse_files,
    frontend::{
        analyzer::{AnalyzedProject, Analyzer},
        source::DiskSources,
        tooling,
    },
    plan::BuildPlan,
    standard_library::StandardLibrary,
};
use std::path::{Path, PathBuf};

fn sysroot() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("library")
}

/// Copy every std source, including per-target subdirectories, so fixtures
/// stay valid as manifest modules are added.
fn copy_std_sources(installed: &Path) {
    fn copy(source: &Path, destination: &Path) {
        std::fs::create_dir_all(destination).unwrap();
        for entry in std::fs::read_dir(source).unwrap() {
            let path = entry.unwrap().path();
            let target = destination.join(path.file_name().unwrap());
            if path.is_dir() {
                copy(&path, &target);
            } else if path.extension().is_some_and(|e| e == "fib") {
                std::fs::copy(&path, target).unwrap();
            }
        }
    }
    copy(&sysroot().join("std"), &installed.join("std"));
}

fn input(source: &str) -> (tempfile::TempDir, BuildPlan) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("main.fib");
    std::fs::write(&path, source).unwrap();
    let mut plan = BuildPlan::single_file(&path).unwrap();
    plan.standard_library = StandardLibrary::at(sysroot());
    (directory, plan)
}

#[test]
fn sentinel_arrays_and_cstrings_lower_with_installed_std() {
    let (_directory, plan) = input(include_str!("programs/sentinels.fib"));
    let project = analyze(&plan).unwrap();
    let ir = fibc::ir::lowering::lower(&project).unwrap();
    assert!(
        ir.types
            .iter()
            .any(|t| matches!(t, fibc::ir::IrType::Array { len: 4, .. }))
    );
}

#[cfg(feature = "llvm")]
#[test]
fn sentinel_storage_and_c_boundary_execute() {
    use fibc::driver::{PipelineGoal, run_pipeline};
    let (directory, mut plan) = input(include_str!("programs/sentinels.fib"));
    let native = directory.path().join("sentinels.c");
    std::fs::write(
        &native,
        r#"
        int check_cstring(const unsigned char *p) {
            return p && p[0] == 'a' && p[1] == 'b' && p[2] == 'c' && p[3] == 0;
        }
        int check_empty(const unsigned char *p) { return p && p[0] == 0; }
        int check_other(const unsigned char *p) { return p && p[0] == 7 && p[1] == 255; }
    "#,
    )
    .unwrap();
    plan.native.sources.push(native);
    run_pipeline(&plan, PipelineGoal::Binary).unwrap();
    assert_eq!(
        std::process::Command::new(&plan.output)
            .status()
            .unwrap()
            .code(),
        Some(42)
    );
}

fn analyze(plan: &BuildPlan) -> anyhow::Result<AnalyzedProject> {
    let prepared = plan.prepare()?;
    Analyzer::new(prepared.clone(), parse_files(&prepared)?)?.analyze()
}

#[test]
fn os_syscall_wrappers_analyze_and_lower() {
    let (_directory, mut plan) = input(include_str!("programs/os_wrappers.fib"));
    // The Linux implementation lowers to raw syscalls on any host.
    plan.standard_library.target_os = Some("linux".into());
    let project = analyze(&plan).unwrap();
    let ir = fibc::ir::lowering::lower(&project).unwrap();
    let syscalls = ir
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .filter(|i| matches!(i.kind, fibc::ir::InstructionKind::Syscall { .. }))
        .count();
    assert!(syscalls >= 60, "only {syscalls} syscall instructions");
}

#[test]
fn portable_os_program_analyzes_and_lowers_for_every_target_family() {
    for family in ["linux", "darwin"] {
        let (_directory, mut plan) = input(include_str!("programs/os_portable.fib"));
        plan.standard_library.target_os = Some(family.into());
        let ir = fibc::ir::lowering::lower(&analyze(&plan).unwrap()).unwrap();
        fibc::ir::verify_ir(&ir).unwrap();
        let syscalls = ir
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(i.kind, fibc::ir::InstructionKind::Syscall { .. }));
        // Linux wraps raw syscalls; macOS goes through libSystem only.
        assert_eq!(syscalls, family == "linux", "{family}");
    }
    // Linux-only names are absent on darwin rather than silently emulated.
    let (_directory, mut plan) = input(
        "fn main() { buffer: [8]uint8 = [0,0,0,0,0,0,0,0]; std::os::getdents64(0, buffer[..]); }",
    );
    plan.standard_library.target_os = Some("darwin".into());
    assert!(format!("{:#}", analyze(&plan).unwrap_err()).contains("getdents64"));
    plan.standard_library.target_os = Some("darwin".into());
    std::fs::write(
        &plan.files[0].path,
        "fn main() { x := std::syscall::GETPID; }",
    )
    .unwrap();
    assert!(format!("{:#}", analyze(&plan).unwrap_err()).contains("syscall"));
}

#[cfg(all(
    feature = "llvm",
    any(
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "aarch64")
    )
))]
#[test]
fn portable_os_program_executes_on_the_host() {
    let (directory, plan) = input(include_str!("programs/os_portable.fib"));
    fibc::driver::run_pipeline(&plan, fibc::driver::PipelineGoal::Binary).unwrap();
    // The program creates and removes its own scratch directory under cwd.
    let output = std::process::Command::new(&plan.output)
        .current_dir(directory.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn syscall_numbers_cover_the_linux_x86_64_table() {
    let numbers = std::fs::read_to_string(sysroot().join("std/syscall.fib")).unwrap();
    let mut values: Vec<u64> = numbers
        .lines()
        .filter_map(|line| line.strip_prefix("pub comp "))
        .map(|line| {
            let (_, value) = line.split_once("= ").unwrap();
            value.trim_end_matches(';').parse().unwrap()
        })
        .collect();
    let count = values.len();
    values.sort_unstable();
    values.dedup();
    assert_eq!(values.len(), count, "duplicate syscall numbers");
    let expected: Vec<u64> = (0..=334).chain(424..=469).collect();
    assert_eq!(values, expected);
}

#[cfg(all(feature = "llvm", target_os = "linux", target_arch = "x86_64"))]
#[test]
fn os_syscall_wrappers_execute() {
    let (directory, plan) = input(include_str!("programs/os_wrappers.fib"));
    fibc::driver::run_pipeline(&plan, fibc::driver::PipelineGoal::Binary).unwrap();
    // The program creates and removes its own scratch directory under cwd.
    let output = std::process::Command::new(&plan.output)
        .current_dir(directory.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn never_returning_exit_sample_lowers() {
    let (_directory, plan) = input(include_str!("../samples/never_exit.fib"));
    fibc::driver::run_pipeline(&plan, fibc::driver::PipelineGoal::Ir).unwrap();
}

#[cfg(all(feature = "llvm", target_os = "linux", target_arch = "x86_64"))]
#[test]
fn never_returning_exit_ends_main_without_a_return() {
    let (_directory, plan) = input(include_str!("../samples/never_exit.fib"));
    fibc::driver::run_pipeline(&plan, fibc::driver::PipelineGoal::Binary).unwrap();
    let status = std::process::Command::new(&plan.output).status().unwrap();
    assert_eq!(status.code(), Some(7));
}

#[test]
fn string_helpers_analyze_and_lower() {
    let (_directory, plan) = input(include_str!("programs/string_helpers.fib"));
    fibc::driver::run_pipeline(&plan, fibc::driver::PipelineGoal::Ir).unwrap();
}

#[cfg(feature = "llvm")]
#[test]
fn string_helpers_execute() {
    for (source, expected) in [
        (include_str!("programs/string_helpers.fib"), ""),
        (
            include_str!("../samples/string_functions.fib"),
            "Hello, fib!",
        ),
    ] {
        let (_directory, plan) = input(source);
        fibc::driver::run_pipeline(&plan, fibc::driver::PipelineGoal::Binary).unwrap();
        let output = std::process::Command::new(&plan.output).output().unwrap();
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert_eq!(output.stdout, expected.as_bytes());
    }
}

#[test]
fn std_helpers_vectors_streams_and_allocators_analyze_and_lower() {
    let (_directory, plan) = input(include_str!("programs/std_helpers.fib"));
    fibc::driver::run_pipeline(&plan, fibc::driver::PipelineGoal::Ir).unwrap();
    for (source, expected) in [
        (
            "comp V := std::vector::Vec(int32); fn main() { v := V.init(std::memory::heap_allocator()); V.push(v&, true); }",
            "incompatible types",
        ),
        (
            "fn main() int32 { return std::result::unwrap_or(int32, bool, std::result::Result(int32, int32).Ok(1), 0); }",
            "incompatible types",
        ),
        (
            "fn main() { x := std::option::is_some(int32, 3); }",
            "incompatible types",
        ),
        (
            "fn main() { p: *uint8 = null; x := std::memory::span(p); }",
            "incompatible types",
        ),
    ] {
        let (_directory, plan) = input(source);
        let error = analyze(&plan).unwrap_err();
        assert!(
            format!("{error:#}").contains(expected),
            "{source}\n{error:#}"
        );
    }
}

#[cfg(feature = "llvm")]
#[test]
fn std_helpers_vectors_streams_and_allocators_execute() {
    let (_directory, plan) = input(include_str!("programs/std_helpers.fib"));
    fibc::driver::run_pipeline(&plan, fibc::driver::PipelineGoal::Binary).unwrap();
    let output = std::process::Command::new(&plan.output).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[cfg(feature = "llvm")]
#[test]
fn expect_on_err_and_none_aborts_with_message() {
    for source in [
        "comp R := std::result::Result(int32, bool); fn main() int32 { return std::result::expect(int32, bool, R.Err(true), \"fatal: boom\"); }",
        "comp O := std::option::Option(int32); fn main() int32 { return std::option::expect(int32, O.None, \"fatal: boom\"); }",
    ] {
        let (_directory, plan) = input(source);
        fibc::driver::run_pipeline(&plan, fibc::driver::PipelineGoal::Binary).unwrap();
        let output = std::process::Command::new(&plan.output).output().unwrap();
        assert_eq!(output.status.code(), Some(134), "{source}\n{output:?}");
        assert_eq!(output.stderr, b"fatal: boom\n");
    }
}

#[test]
fn generic_maps_validate_specialization_and_reject_mismatched_types() {
    let (_directory, plan) = input(include_str!("../samples/generic_map.fib"));
    fibc::driver::run_pipeline(&plan, fibc::driver::PipelineGoal::Ir).unwrap();
    for source in [
        "fn eq(a: int32, b: int32) bool { return a == b; } fn main() { m := std::map::init(int32, int64, eq); std::map::insert(int32, bool, m&, 1, true); }",
        "fn eq(a: bool, b: bool) bool { return a == b; } fn main() { m := std::map::init(int32, int64, eq); }",
        "fn value() int32 { return 1; } fn main() { x: value(); }",
    ] {
        let (_directory, plan) = input(source);
        let error = analyze(&plan).unwrap_err();
        assert!(
            format!("{error:#}").contains("incompatible types")
                || format!("{error:#}").contains("requires a type value"),
            "{source}\n{error:#}"
        );
    }
}

#[cfg(feature = "llvm")]
#[test]
fn generic_maps_execute_with_scalar_and_struct_keys_and_values() {
    for source in [
        include_str!("../samples/generic_map.fib"),
        r#"
        comp Key := struct { number: int32, ignored: bool };
        comp Value := struct { answer: int64, flag: bool };
        fn equal(a: Key, b: Key) bool { return a.number == b.number; }
        fn put(m: *std::map::Map(Key, Value), key: Key, value: Value) bool {
            return std::result::is_ok(void, std::memory::AllocError, std::map::insert(Key, Value, m, key, value));
        }
        fn main() int32 {
            m := std::map::init(Key, Value, equal);
            defer std::map::deinit(Key, Value, m&);
            i := 0;
            while (i < 20) {
                if !put(m&, Key { number: i, ignored: false }, Value { answer: i as int64, flag: false }) { return 1; };
                i = i + 1;
            };
            if !put(m&, Key { number: 7, ignored: true }, Value { answer: 42, flag: true }) { return 2; };
            if m.len != 20 { return 3; };
            switch (std::map::get(Key, Value, m, Key { number: 7, ignored: false })) {
                when .Some(value) { if value.answer != 42 || !value.flag { return 4; }; }
                when .None { return 5; }
            };
            switch (std::map::get(Key, Value, m, Key { number: 25, ignored: false })) {
                when .Some(value) { return 6; }
                when .None {}
            };
            std::map::deinit(Key, Value, m&);
            if !put(m&, Key { number: 1, ignored: true }, Value { answer: 42, flag: true }) { return 7; };
            return 42;
        }
    "#,
    ] {
        let (_directory, plan) = input(source);
        fibc::driver::run_pipeline(&plan, fibc::driver::PipelineGoal::Binary)
            .unwrap_or_else(|e| panic!("{source}\n{e:#}"));
        assert_eq!(
            std::process::Command::new(&plan.output)
                .status()
                .unwrap()
                .code(),
            Some(42)
        );
    }
}

#[test]
fn process_entry_contracts_validate_and_lower() {
    use fibc::driver::{PipelineGoal, run_pipeline};
    for source in [
        "fn main() {}",
        "comp R := std::result::Result(void, int32); fn main() R { return R.Ok; }",
        "comp R := std::result::Result(int32, void); fn main() R { return R.Ok(42); }",
        "fn main(args: std::process::Arguments) int32 { return args.len as int32; }",
        "comp M := std::map::Map(const []uint8, const [:0]uint8); fn main(env: M) int32 { return env.len as int32; }",
        "fn main(args: std::process::Arguments) int32 { if args.len < 2 { return 0; }; switch (std::os::access(std::memory::slice_data(args[1]), std::os::F_OK)) { when .Ok { return 0; } when .Err(error) { return error; } }; }",
        include_str!("../samples/process_entry.fib"),
        "fn __fib_process_entry() {} fn main(args: std::process::Arguments) {}",
    ] {
        let (_directory, plan) = input(source);
        run_pipeline(&plan, PipelineGoal::Ir).unwrap_or_else(|e| panic!("{source}\n{e:#}"));
    }
    for (source, expected) in [
        ("fn main(x: int32) int32 { return x; }", "main parameters"),
        (
            "fn main(a: std::process::Arguments, b: std::process::Arguments) {}",
            "main parameters",
        ),
        (
            "fn main(e: std::process::Environment, a: std::process::Arguments) {}",
            "main parameters",
        ),
        ("fn main() int64 { return 0; }", "must return"),
        (
            "comp R := enum { Ok, Err }; fn main() R { return R.Ok; }",
            "must return",
        ),
        (
            "comp R := std::result::Result(bool, void); fn main() R { return R.Ok(true); }",
            "success type",
        ),
        (
            "comp fn main() int32 { return 0; }",
            "ordinary Fib definition",
        ),
    ] {
        let (_directory, plan) = input(source);
        let error = run_pipeline(&plan, PipelineGoal::Check).unwrap_err();
        assert!(
            format!("{error:#}").contains(expected),
            "{source}\n{error:#}"
        );
        assert!(!plan.output.exists());
    }
}

#[cfg(feature = "llvm")]
#[test]
fn process_entries_execute_with_arguments_environment_and_result_status() {
    use fibc::driver::{PipelineGoal, run_pipeline};
    for (source, arguments, environment, status) in [
        (
            include_str!("../samples/process_entry.fib"),
            vec![],
            vec![("FIB_EXIT", "42")],
            42,
        ),
        (
            include_str!("../samples/process_entry.fib"),
            vec!["argument"],
            vec![],
            0,
        ),
        (
            include_str!("../samples/process_entry.fib"),
            vec![],
            vec![],
            1,
        ),
        ("fn main() {}", vec![], vec![], 0),
        (
            "comp R := std::result::Result(void, const []uint8); fn main() R { return R.Err(\"failed\"[..]); }",
            vec![],
            vec![],
            1,
        ),
        (
            "comp R := std::result::Result(int32, void); calls := 0; fn main() R { if calls == 0 { calls = 1; return main(); }; return R.Ok(42); }",
            vec![],
            vec![],
            42,
        ),
        (
            "fn main(args: std::process::Arguments, env: std::process::Environment) int32 { switch (std::map::insert(const []uint8, const [:0]uint8, env&, \"new\", \"value\")) { when .Ok {} when .Err { return 1; } }; std::map::deinit(const []uint8, const [:0]uint8, env&); if args.len > 0 { return 42; }; return 1; }",
            vec![],
            vec![],
            42,
        ),
        (
            // Arguments are NUL-terminated, so they pass directly to path syscalls.
            "fn main(args: std::process::Arguments) int32 { switch (std::os::access(std::memory::slice_data(args[0]), std::os::X_OK)) { when .Ok { return 42; } when .Err(error) { return error; } }; }",
            vec![],
            vec![],
            42,
        ),
        (
            "comp R := std::result::Result(void, int32); fn main() R { return R.Ok; }",
            vec![],
            vec![],
            0,
        ),
        (
            "comp R := std::result::Result(void, int32); fn main() R { return R.Err(42); }",
            vec![],
            vec![],
            1,
        ),
        (
            "comp R := std::result::Result(int32, void); fn main() R { return R.Ok(42); }",
            vec![],
            vec![],
            42,
        ),
        (
            "fn main(args: std::process::Arguments) int32 { if args.len == 1 && args[0].len > 0 { return 42; }; return 1; }",
            vec![],
            vec![],
            42,
        ),
        (
            "fn main(args: std::process::Arguments) int32 { if args.len == 4 && args[1].len == 0 && args[2].len == 3 && args[2][1] == 32 && args[3].len == 2 { return 42; }; return 1; }",
            vec!["", "a b", "é"],
            vec![],
            42,
        ),
        (
            "fn main(env: std::process::Environment) int32 { if env.len == 0 { return 42; }; return 1; }",
            vec![],
            vec![],
            42,
        ),
        (
            r#"comp R := std::result::Result(int32, void);
            fn main(args: std::process::Arguments, env: std::process::Environment) R {
                if args.len != 2 || args[1].len != 0 || env.len != 2 { return R.Err; };
                switch (std::process::getenv(env, "EMPTY")) {
                    when .Some(value) { if value.len != 0 { return R.Err; }; }
                    when .None { return R.Err; }
                };
                switch (std::process::getenv(env, "VALUE")) {
                    when .Some(value) { if value.len == 3 && value[1] == '=' { return R.Ok(42); }; }
                    when .None {}
                };
                return R.Err;
            }"#,
            vec![""],
            vec![("EMPTY", ""), ("VALUE", "a=b")],
            42,
        ),
    ] {
        let (_directory, plan) = input(source);
        run_pipeline(&plan, PipelineGoal::Binary).unwrap_or_else(|e| panic!("{source}\n{e:#}"));
        let output = std::process::Command::new(&plan.output)
            .args(arguments)
            .env_clear()
            .envs(environment)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(status), "{source}\n{output:?}");
    }
}

#[cfg(feature = "llvm")]
#[test]
fn environment_vectors_split_once_replace_duplicates_and_skip_malformed_entries() {
    let source = r#"
        fn main() int32 {
            first: [4]uint8 = [75, 61, 49, 0];
            malformed: [2]uint8 = [88, 0];
            replacement: [6]uint8 = [75, 61, 50, 61, 255, 0];
            empty: [3]uint8 = [69, 61, 0];
            vector := [first[0]&, malformed[0]&, replacement[0]&, empty[0]&, null as *uint8];
            env := std::process::environment(vector[0]&);
            defer std::map::deinit(const []uint8, const [:0]uint8, env&);
            if env.len != 2 { return 1; };
            switch (std::process::getenv(env, "K")) {
                when .Some(value) { if value.len != 3 || value[0] != 50 || value[1] != 61 || value[2] != 255 { return 2; }; }
                when .None { return 3; }
            };
            switch (std::process::getenv(env, "E")) {
                when .Some(value) { if value.len != 0 { return 4; }; }
                when .None { return 5; }
            };
            return 42;
        }
    "#;
    let (_directory, plan) = input(source);
    fibc::driver::run_pipeline(&plan, fibc::driver::PipelineGoal::Binary).unwrap();
    assert_eq!(
        std::process::Command::new(&plan.output)
            .status()
            .unwrap()
            .code(),
        Some(42)
    );
}

#[cfg(feature = "llvm")]
#[test]
fn byte_string_map_grows_replaces_and_releases_storage() {
    let source = r#"
        comp Strings := std::map::Map(const []uint8, const []uint8);
        fn put(map: *Strings, key: const []uint8, value: const []uint8) bool {
            return std::result::is_ok(void, std::memory::AllocError,
                std::map::insert(const []uint8, const []uint8, map, key, value));
        }
        fn main() int32 {
            map := std::map::init(const []uint8, const []uint8, std::string::eq);
            defer std::map::deinit(const []uint8, const []uint8, map&);
            if !put(map&, ""[..], ""[..]) { return 1; };
            keys := ["a"[..], "b"[..], "c"[..], "d"[..], "e"[..], "f"[..], "g"[..], "h"[..], "i"[..]];
            i: usize = 0;
            while (i < keys.len) {
                if !put(map&, keys[i], "old"[..]) { return 2; };
                i = i + 1;
            };
            if !put(map&, "a"[..], "new"[..]) || map.len != 10 { return 3; };
            switch (std::map::get(const []uint8, const []uint8, map, "a"[..])) {
                when .Some(value) { if value[0] != 'n' { return 4; }; }
                when .None { return 5; }
            };
            switch (std::map::get(const []uint8, const []uint8, map, "absent"[..])) {
                when .Some(value) { return 6; }
                when .None {}
            };
            std::map::deinit(const []uint8, const []uint8, map&);
            if map.len != 0 { return 7; };
            if !put(map&, "reused"[..], "yes"[..]) { return 8; };
            return 42;
        }
    "#;
    let (_directory, plan) = input(source);
    fibc::driver::run_pipeline(&plan, fibc::driver::PipelineGoal::Binary).unwrap();
    assert_eq!(
        std::process::Command::new(&plan.output)
            .status()
            .unwrap()
            .code(),
        Some(42)
    );
}

#[test]
fn memory_byte_operations_accept_writable_destinations_and_readonly_sources() {
    let (_directory, plan) = input(include_str!("../samples/memory.fib"));
    fibc::ir::verify_ir(&fibc::ir::lowering::lower(&analyze(&plan).unwrap()).unwrap()).unwrap();
    for source in [
        "fn main() { std::memory::memset(\"hi\"[..], 0); }",
        "fn main() { std::memory::memcpy(\"hi\"[..], \"ok\"[..]); }",
        "fn main() { a: [1]uint16 = [0]; std::memory::memset(a[..], 0); }",
    ] {
        let (_directory, plan) = input(source);
        let error = analyze(&plan).unwrap_err();
        assert!(
            format!("{error:#}").contains("incompatible types"),
            "{error:#}"
        );
    }
}

#[cfg(feature = "llvm")]
#[test]
fn memory_byte_operations_execute_and_trap_for_short_destinations() {
    for (source, expected) in [
        (include_str!("../samples/memory.fib"), Some(42)),
        (
            "fn main() int32 { a: [1]uint8 = [0]; std::memory::memcpy(a[..], \"hi\"[..]); return 1; }",
            None,
        ),
    ] {
        let (_directory, plan) = input(source);
        fibc::driver::run_pipeline(&plan, fibc::driver::PipelineGoal::Binary).unwrap();
        let output = std::process::Command::new(&plan.output).output().unwrap();
        assert_eq!(output.status.code(), expected, "{source}\n{output:?}");
    }
}

#[test]
fn formatting_sources_analyze_and_lower() {
    for source in [
        include_str!("../samples/hello_world.fib"),
        include_str!("../samples/formatting.fib"),
        include_str!("programs/formatting.fib"),
        include_str!("programs/formatting_buffer.fib"),
    ] {
        let (_directory, plan) = input(source);
        fibc::ir::verify_ir(&fibc::ir::lowering::lower(&analyze(&plan).unwrap()).unwrap()).unwrap();
    }
}

#[cfg(feature = "llvm")]
#[test]
fn formatting_executes_with_exact_bytes_counts_and_errors() {
    for (source, stdout, stderr) in [
        (
            include_str!("../samples/hello_world.fib"),
            &b"Hello, World!\n"[..],
            &b""[..],
        ),
        (
            include_str!("../samples/formatting.fib"),
            &b"Hello Alice, score: -42, max: 18446744073709551615, active: true {ok}\n"[..],
            &b""[..],
        ),
        (
            include_str!("programs/formatting.fib"),
            &b"-9223372036854775808|9223372036854775807|0|0|false||{raw}\0\n"[..],
            &b"{stderr}"[..],
        ),
        (
            include_str!("programs/formatting_buffer.fib"),
            &b""[..],
            &b""[..],
        ),
    ] {
        let (_directory, plan) = input(source);
        fibc::driver::run_pipeline(&plan, fibc::driver::PipelineGoal::Binary).unwrap();
        let output = std::process::Command::new(&plan.output).output().unwrap();
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert_eq!(output.stdout, stdout);
        assert_eq!(output.stderr, stderr);
    }
}

#[test]
#[cfg(feature = "llvm")]
fn formatting_write_all_retries_interruptions_and_short_writes() {
    let source = r#"
        comp io := std::io;
        fn main() int32 {
            switch (io::write_all(io::stdout(), "abcde"[..])) {
                when .Err(e) { return 1; }
                when .Ok(n) { if n != 5 || std::os::test_calls() != 4 { return 2; }; }
            };
            std::os::test_reset(1);
            switch (io::write_all(io::stdout(), "x"[..])) {
                when .Ok(n) { return 3; }
                when .Err(errno) { if errno != std::os::EIO { return 4; }; }
            };
            std::os::test_reset(2);
            switch (io::write_all(io::stdout(), "x"[..])) {
                when .Ok(n) { return 7; }
                when .Err(errno) { if errno != std::os::ENOSPC { return 8; }; }
            };
            // print reports the same errno through the formatting error type.
            switch (io::print("x"[..])) {
                when .Ok(n) { return 9; }
                when .Err(e) {
                    switch (e) {
                        when .Io(errno) { if errno != std::os::ENOSPC { return 10; }; }
                        when .InvalidFormat { return 11; }
                        when .ArgumentCount { return 12; }
                        when .BufferTooSmall { return 13; }
                    };
                }
            };
            return 0;
        }
    "#;
    let (directory, mut plan) = input(source);
    let installed = directory.path().join("sysroot");
    std::fs::create_dir_all(installed.join("std")).unwrap();
    std::fs::copy(
        sysroot().join("sysroot.toml"),
        installed.join("sysroot.toml"),
    )
    .unwrap();
    copy_std_sources(&installed);
    // Script the write boundary to force otherwise timing-dependent outcomes.
    // Patch whichever std::os implementation the host target loads.
    let path = installed.join(if cfg!(target_os = "macos") {
        "std/darwin/os.fib"
    } else {
        "std/os.fib"
    });
    let mut os = std::fs::read_to_string(&path).unwrap();
    let start = os.find("pub fn write(").unwrap();
    let end = os[start..].find("pub fn close(").unwrap() + start;
    os.replace_range(
        start..end,
        r#"
        test_mode: int32 = 0;
        calls: usize = 0;
        sent: usize = 0;
        pub fn test_reset(mode: int32) { test_mode = mode; calls = 0; sent = 0; }
        pub fn test_calls() usize { return calls; }
        pub fn write(fd: int32, buffer: const []uint8) CountResult {
            calls = calls + 1;
            if test_mode == 1 { return CountResult.Ok(0); };
            if test_mode == 2 { return CountResult.Err(28); };
            if calls == 1 { return CountResult.Err(4); };
            if buffer.len != 5 - sent || buffer[0] != 97 + (sent as uint8) {
                return CountResult.Err(22);
            };
            count: usize = 2;
            if buffer.len < count { count = buffer.len; };
            sent = sent + count;
            return CountResult.Ok(count);
        }
    "#,
    );
    std::fs::write(path, os).unwrap();
    plan.standard_library = StandardLibrary::at(installed);
    fibc::driver::run_pipeline(&plan, fibc::driver::PipelineGoal::Binary).unwrap();
    let output = std::process::Command::new(&plan.output).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn installed_sources_prepare_idempotently_and_lower_with_canonical_aliases() {
    let (_directory, plan) = input(
        r#"
        comp result := std::result;
        comp A := result::Result(int32, bool);
        comp B := std::result::Result(int32, bool);
        comp Other := result::Result(int32, int32);
        fn identity(value: B) A { return value; }
        fn main() int32 {
            value := identity(A.Ok(42));
            switch (value) {
                when .Ok(payload) { return payload; }
                when .Err(payload) { return payload as int32; }
            };
        }
    "#,
    );
    let prepared = plan.prepare().unwrap();
    let again = prepared.prepare().unwrap();
    assert_eq!(prepared.files, again.files);
    assert_eq!(prepared.entry, plan.entry);
    assert_eq!(prepared.packages.len(), 2);
    let project = analyze(&prepared).unwrap();
    fibc::ir::verify_ir(&fibc::ir::lowering::lower(&project).unwrap()).unwrap();
    assert!(
        project
            .builtins
            .values()
            .any(|b| b.syscall_arity() == Some(6))
    );
    let report = tooling::analyze_plan(&plan, &DiskSources, false);
    assert!(
        report
            .documents
            .values()
            .all(|d| d.diagnostics.iter().all(|e| e.warning)),
        "{:?}",
        report
            .documents
            .iter()
            .map(|(p, d)| (
                p,
                d.diagnostics
                    .iter()
                    .map(|e| e.diagnostic.to_string())
                    .collect::<Vec<_>>()
            ))
            .collect::<Vec<_>>()
    );
}

#[test]
fn loading_std_preserves_flat_plan_names_and_existing_source_ids() {
    use fibc::plan::{FileId, SourceFile};
    let (directory, mut plan) =
        input("fn main() int32 { return shortened::value() + root::nested::helper::value(); }");
    std::fs::create_dir(directory.path().join("nested")).unwrap();
    // The entry path is canonical; match it where temp dirs are symlinks (macOS).
    let path = directory
        .path()
        .canonicalize()
        .unwrap()
        .join("nested/helper.fib");
    std::fs::write(&path, "pub(root) fn value() int32 { return 21; }").unwrap();
    plan.files.push(SourceFile {
        id: FileId(1),
        name: "shortened".to_string().into(),
        path: path.clone(),
    });
    let prepared = plan.prepare().unwrap();
    assert_eq!(prepared.files[1].id, FileId(1));
    assert_eq!(prepared.files[1].path, path);
    fibc::ir::lowering::lower(&analyze(&prepared).unwrap()).unwrap();
}

#[test]
fn directly_opened_std_modules_keep_their_package_and_document_identity() {
    // Every std file, including per-target implementations such as
    // darwin/os.fib, opens as a member of some target's source set.
    let mut modules: Vec<PathBuf> = Vec::new();
    let mut directories = vec![sysroot().join("std")];
    while let Some(directory) = directories.pop() {
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path().canonicalize().unwrap();
            if path.is_dir() {
                directories.push(path);
            } else if path.extension().is_some_and(|e| e == "fib") {
                modules.push(path);
            }
        }
    }
    let top_level = modules
        .iter()
        .filter(|path| path.parent() == sysroot().join("std").canonicalize().ok().as_deref())
        .count();
    for path in &modules {
        let path = path.clone();
        // The owning source tree takes precedence over the server's installation.
        let plan = tooling::standalone_plan(&path)
            .with_standard_library(StandardLibrary::at("/missing/installed/sysroot"));
        let prepared = plan.prepare().unwrap();
        assert_eq!(prepared.entry, plan.entry);
        // Each target loads as many modules as there are top-level files: Linux
        // omits libc, darwin swaps syscall for darwin/libc.
        assert_eq!(prepared.files.len(), top_level, "{}", path.display());
        assert_eq!(prepared.packages.len(), 1);
        assert_eq!(prepared.files.iter().filter(|f| f.path == path).count(), 1);
        assert!(prepared.packages[0].files.contains(&plan.entry));
        assert_eq!(prepared.prepare().unwrap().files, prepared.files);
        analyze(&prepared).unwrap();
        let report = tooling::analyze_plan(&plan, &DiskSources, false);
        assert!(
            report.documents[&path].diagnostics.is_empty(),
            "{}: {:?}",
            path.display(),
            report.documents[&path].diagnostics
        );
    }
}

#[test]
fn invalid_syscall_arguments_and_private_conversions_are_rejected() {
    for (body, expected) in [
        (
            "return std::os::syscall0(39, 0) as int32;",
            "argument count",
        ),
        ("return std::os::syscall1(39, true) as int32;", "uint64"),
        (
            "x: uint32 = 0; return std::os::syscall1(39, x) as int32;",
            "uint64",
        ),
        (
            "p := std::os_abi::pointer_from_word(0); return 0;",
            "private",
        ),
        (
            "p: *uint8 = null; x := std::os_abi::pointer_to_word(p); return 0;",
            "private",
        ),
        (
            "p: *uint8 = null; x := std::os_abi::terminated_from_parts(p, 0); return 0;",
            "private",
        ),
        ("x := std::memory::c_string_at(0); return 0;", "private"),
        ("p: *uint8 = null; x := p as uint64; return 0;", "cast"),
        ("return memory::unchecked_deref();", "unresolved"),
        ("std := 1; return std;", "cannot be shadowed"),
        (
            "comp R := std::result::Result(int32, bool); value := R.Err(1); return 0;",
            "incompatible",
        ),
    ] {
        let (_directory, plan) = input(&format!("fn main() int32 {{ {body} }}"));
        let error = analyze(&plan).unwrap_err();
        assert!(format!("{error:#}").contains(expected), "{body}: {error:#}");
    }
}

#[test]
fn sysroot_failures_and_no_std_are_explicit() {
    let (directory, mut plan) = input("fn main() int32 { return 0; }");
    plan.standard_library = StandardLibrary::at(directory.path());
    assert!(format!("{:#}", plan.prepare().unwrap_err()).contains("sysroot"));
    std::fs::write(
        directory.path().join("sysroot.toml"),
        "compiler_version = 'wrong'\ndeclaration_revision = 1\nsources = []\n",
    )
    .unwrap();
    assert!(format!("{:#}", plan.prepare().unwrap_err()).contains("incompatible"));
    plan.standard_library = StandardLibrary::disabled();
    assert_eq!(plan.prepare().unwrap().files.len(), 1);
    analyze(&plan).unwrap();
    std::fs::write(
        &plan.files[0].path,
        "fn main() int32 { return std::os::syscall0(39) as int32; }",
    )
    .unwrap();
    assert!(analyze(&plan).is_err());
}

#[test]
fn runtime_syscalls_are_rejected_by_the_build_interpreter() {
    let (_directory, plan) = input("fn build() int32 { std::os::syscall0(39); return 0; }");
    let ir = fibc::driver::compile_build_file_to_ir(&plan).unwrap();
    let mut builder = fibc::plan::PlanBuilder::from(plan.files[0].path.clone());
    let error = fibc::ir_interpreter::interpret_ir(ir, &mut builder).unwrap_err();
    assert!(
        error.to_string().contains("syscalls cannot be interpreted"),
        "{error:#}"
    );
    let prepared = plan.prepare().unwrap();
    let project = Analyzer::new(prepared.clone(), parse_files(&prepared).unwrap())
        .unwrap()
        .with_synthetic_module(fibc::builtins::SyntheticModule::BUILD_API)
        .unwrap()
        .analyze()
        .unwrap();
    for (id, builtin) in &project.builtins {
        if !builtin.is_intrinsic() {
            assert!(
                !prepared.files.iter().any(|f| f.id == id.file_id),
                "synthetic build operations must not overwrite a source module"
            );
        }
    }
}

#[test]
fn intrinsic_metadata_and_syscall_ir_are_independently_validated() {
    use fibc::ir::*;
    let (_directory, plan) = input("fn main() int32 { return std::os::syscall0(39) as int32; }");
    let project = analyze(&plan).unwrap();
    let mut forged = project.clone();
    let id = *forged
        .builtins
        .iter()
        .find(|(_, b)| b.syscall_arity() == Some(0))
        .unwrap()
        .0;
    forged.symbols.compiler_declarations.remove(&id);
    assert!(fibc::frontend::analyzer::validate_analysis(&forged).is_err());
    forged.builtins.remove(&id);
    assert!(fibc::frontend::analyzer::validate_analysis(&forged).is_err());
    let ir = fibc::ir::lowering::lower(&project).unwrap();
    for corruption in 0..3 {
        let mut bad = ir.clone();
        let boolean = bad
            .types
            .iter()
            .position(|t| matches!(t, IrType::Bool))
            .unwrap();
        let bool_constant = Operand::Constant(ConstantId(bad.constants.len()));
        bad.constants.push(Constant {
            ty: TypeId(boolean),
            kind: ConstantKind::Bool(true),
        });
        let instruction = bad
            .blocks
            .iter_mut()
            .flat_map(|b| &mut b.instructions)
            .find(|i| matches!(i.kind, InstructionKind::Syscall { .. }))
            .unwrap();
        let InstructionKind::Syscall { number, args } = &mut instruction.kind else {
            unreachable!()
        };
        match corruption {
            0 => *number = bool_constant,
            1 => args.push(bool_constant),
            _ => *args = vec![*number; 7],
        }
        assert!(verify_ir(&bad).is_err());
    }
    for invalid_source in [false, true] {
        let mut bad = ir.clone();
        let boolean = TypeId(
            bad.types
                .iter()
                .position(|t| matches!(t, IrType::Bool))
                .unwrap(),
        );
        let constant = Operand::Constant(ConstantId(bad.constants.len()));
        bad.constants.push(Constant {
            ty: boolean,
            kind: ConstantKind::Bool(true),
        });
        let instruction = bad
            .blocks
            .iter_mut()
            .flat_map(|b| &mut b.instructions)
            .find(|i| {
                matches!(
                    i.kind,
                    InstructionKind::Cast {
                        kind: CastKind::PointerToInteger,
                        ..
                    }
                )
            })
            .unwrap();
        if invalid_source {
            let InstructionKind::Cast { operand, .. } = &mut instruction.kind else {
                unreachable!()
            };
            *operand = constant;
        } else {
            bad.values[instruction.results[0].0].ty = boolean;
        }
        assert!(
            verify_ir(&bad)
                .unwrap_err()
                .to_string()
                .contains("pointer-to-integer")
        );
    }
}

#[test]
fn prelude_exports_are_ordinary_aliases_and_collisions_are_errors() {
    let (directory, mut plan) = input(
        "fn main() int32 { comp R := Result(int32, bool); value := R.Ok(42); switch (value) { when .Ok(p) { return p; } when .Err(p) { return 0; } }; }",
    );
    let installed = directory.path().join("sysroot");
    std::fs::create_dir_all(installed.join("std")).unwrap();
    std::fs::copy(
        sysroot().join("sysroot.toml"),
        installed.join("sysroot.toml"),
    )
    .unwrap();
    copy_std_sources(&installed);
    std::fs::write(
        installed.join("std/prelude.fib"),
        "pub comp Result := root::result::Result;",
    )
    .unwrap();
    plan.standard_library = StandardLibrary::at(&installed);
    let project = analyze(&plan).unwrap();
    let implicit = project
        .units
        .iter()
        .find(|u| u.source_file_id == plan.entry)
        .unwrap()
        .ast
        .statements
        .last()
        .unwrap();
    let fibc::frontend::ast::statement::StatementKind::VariableDeclaration(alias) = &implicit.kind
    else {
        panic!("implicit alias");
    };
    assert!(
        alias.annotation.source_span.is_none(),
        "implicit exports have no textual location in the consumer"
    );
    fibc::ir::lowering::lower(&project).unwrap();
    std::fs::write(
        &plan.files[0].path,
        "comp Result := int32; fn main() int32 { return 0; }",
    )
    .unwrap();
    assert!(format!("{:#}", analyze(&plan).unwrap_err()).contains("duplicate identifier"));
    std::fs::write(
        installed.join("std/prelude.fib"),
        "pub comp std := root::memory;",
    )
    .unwrap();
    std::fs::write(&plan.files[0].path, "fn main() int32 { return 0; }").unwrap();
    assert!(format!("{:#}", analyze(&plan).unwrap_err()).contains("cannot be shadowed"));
}

// Linux x86-64: exercises sbrk adjacency and raw setrlimit/brk syscalls.
#[cfg(all(feature = "llvm", target_os = "linux", target_arch = "x86_64"))]
#[test]
fn allocator_and_sbrk_execute_against_installed_sources() {
    for source in [
        include_str!("../samples/allocator.fib"),
        include_str!("programs/sbrk.fib"),
        r#"
        comp m := std::memory;
        // Failed allocations become empty slices so checks can compare lengths.
        fn take(T: type, count: usize) []T {
            empty: *T = null;
            return std::result::unwrap_or([]T, m::AllocError, m::alloc(T, count), m::slice_from_parts(empty, 0));
        }
        fn released(T: type, items: []T) bool {
            return std::result::is_ok(void, m::AllocError, m::free(T, items));
        }
        fn main() int32 {
            // Force an unaligned initial break before the allocator starts.
            switch (std::os::sbrk(1)) { when .Ok(p) {} when .Err(e) { return 1; } };
            big := take(uint8, 256);
            guard := take(uint128, 2);
            if big.len != 256 || guard.len != 2 { return 2; };
            guard[0] = 42;
            original := m::slice_data(big);
            if !released(uint8, big) { return 9; };
            left := take(uint8, 64);
            right := take(uint8, 64);
            if left.len != 64 || right.len != 64 || m::slice_data(left) != original { return 3; };
            if m::slice_data(left) == m::slice_data(right) { return 4; };
            left[63] = 41;
            right[0] = 42;
            if left[63] != 41 || right[0] != 42 || guard[0] != 42 { return 5; };
            if !released(uint8, right) || !released(uint8, left) { return 10; };
            merged := take(uint8, 256);
            if merged.len != 256 || m::slice_data(merged) != original { return 6; };
            if !released(uint8, merged) || !released(uint128, guard) { return 11; };
            // Adjacent independently requested regions also coalesce.
            larger := take(uint8, 300);
            if larger.len != 300 || m::slice_data(larger) != original { return 7; };
            larger[299] = 42;
            // An unrelated pointer is reported and must not corrupt the list.
            local := 1;
            if released(int32, m::slice_from_parts(local&, 1)) { return 12; };
            if !released(uint8, larger) { return 13; };
            // Double frees are reported too.
            if released(uint8, larger) { return 14; };
            again := take(uint8, 300);
            if again.len != 300 || m::slice_data(again) != original { return 8; };
            return 42;
        }
        "#,
        r#"
        comp m := std::memory;
        fn take(T: type, count: usize) []T {
            empty: *T = null;
            return std::result::unwrap_or([]T, m::AllocError, m::alloc(T, count), m::slice_from_parts(empty, 0));
        }
        fn main() int32 {
            // Reject multiplication beyond the address space before asking Linux.
            huge := take([1073741824]uint64, 4294967295);
            if huge.len != 0 { return 1; };
            live := take(uint8, 16);
            if live.len != 16 { return 4; };
            live[0] = 42;
            // setrlimit(RLIMIT_DATA, {0, 0}) makes brk growth fail deterministically.
            limits: [2]uint64 = [0, 0];
            if std::os::syscall2(160, 2, limits&) != 0 { return 2; };
            switch (m::alloc(uint8, 32)) {
                when .Ok(bytes) { return 3; }
                when .Err(error) {
                    switch (error) {
                        when .NotEnoughMemory {}
                        when .MapFailed { return 7; }
                        when .InvalidAlignment { return 7; }
                        when .UnmapFailed { return 7; }
                        when .InvalidFree { return 7; }
                    };
                }
            };
            if live[0] != 42 { return 5; };
            original := m::slice_data(live);
            m::free(uint8, live);
            reused := take(uint8, 16);
            if reused.len != 16 || m::slice_data(reused) != original { return 6; };
            return 42;
        }
        "#,
    ] {
        let (_directory, plan) = input(source);
        fibc::driver::run_pipeline(&plan, fibc::driver::PipelineGoal::Binary)
            .unwrap_or_else(|e| panic!("{source}\n{e:#}"));
        let output = std::process::Command::new(&plan.output).output().unwrap();
        assert_eq!(output.status.code(), Some(42), "{source}\n{output:?}");
    }
}

#[cfg(all(feature = "llvm", target_os = "linux", target_arch = "x86_64"))]
#[test]
fn syscall_wrappers_execute_without_a_native_shim() {
    use fibc::driver::{PipelineGoal, run_pipeline};
    let (_directory, plan) = input(include_str!("programs/standard_library.fib"));
    run_pipeline(&plan, PipelineGoal::Binary).unwrap();
    let output = std::process::Command::new(&plan.output).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(output.stdout, b"std syscalls\n");
}

#[cfg(all(feature = "llvm", target_os = "linux", target_arch = "x86_64"))]
#[test]
fn syscall_clobbers_survive_optimization_and_other_targets_are_rejected() {
    use fibc::backend::{emit_llvm, lowering::llvm_lower::host_target};
    use inkwell::{
        OptimizationLevel,
        context::Context,
        memory_buffer::MemoryBuffer,
        passes::PassBuilderOptions,
        targets::{CodeModel, FileType, RelocMode, Target, TargetTriple},
    };
    let (directory, plan) = input(include_str!("programs/standard_library.fib"));
    let ir = fibc::ir::lowering::lower(&analyze(&plan).unwrap()).unwrap();
    let target = host_target().unwrap();
    let mut unsupported = target.clone();
    unsupported.triple = "x86_64-unknown-freebsd".into();
    assert!(
        emit_llvm(&ir, &unsupported)
            .unwrap_err()
            .to_string()
            .contains("Linux x86-64")
    );
    unsupported.triple = "aarch64-unknown-linux-gnu".into();
    assert!(emit_llvm(&ir, &unsupported).is_err());
    let artifact = emit_llvm(&ir, &target).unwrap();
    assert!(artifact.ir_text().contains("asm sideeffect"));
    for constraint in ["{r10}", "{r8}", "{r9}", "~{memory}", "~{rcx}", "~{r11}"] {
        assert!(artifact.ir_text().contains(constraint), "{constraint}");
    }
    let context = Context::create();
    let buffer =
        MemoryBuffer::create_from_memory_range_copy(artifact.ir_text().as_bytes(), "syscalls");
    let module = context.create_module_from_ir(buffer).unwrap();
    let triple = TargetTriple::create(&target.triple);
    let machine = Target::from_triple(&triple)
        .unwrap()
        .create_target_machine(
            &triple,
            "generic",
            "",
            OptimizationLevel::Aggressive,
            RelocMode::PIC,
            CodeModel::Default,
        )
        .unwrap();
    module
        .run_passes("default<O2>", &machine, PassBuilderOptions::create())
        .unwrap();
    module.verify().unwrap();
    let object = directory.path().join("optimized.o");
    machine
        .write_to_file(&module, FileType::Object, &object)
        .unwrap();
    let binary = directory.path().join("optimized");
    let link = std::process::Command::new("cc")
        .arg(&object)
        .arg("-o")
        .arg(&binary)
        .output()
        .unwrap();
    assert!(link.status.success(), "{link:?}");
    let output = std::process::Command::new(binary).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(output.stdout, b"std syscalls\n");
}

// Linux-only: on macOS, read() into a private mapping of the same file can
// deadlock in the kernel, leaving an unkillable process.
#[cfg(all(feature = "llvm", target_os = "linux"))]
#[test]
fn file_backed_mapping_and_short_reads_preserve_kernel_results() {
    let (directory, mut plan) = input("");
    let file = directory.path().join("bytes");
    std::fs::write(&file, [42_u8, 43, 44]).unwrap();
    let source = r#"
        comp os := std::os;
        fn main() int32 {
            switch (os::openat(-100, c"FILE", 0, 0)) {
                when .Ok(file) {
                    defer { os::close(file); };
                    switch (os::mmap(null, 4096, 3, 2, file, 0)) {
                        when .Ok(mapping) {
                            pointer := mapping;
                            defer { os::munmap(pointer, 4096); };
                            if (pointer*) != 42 { return 1; };
                            buffer := std::memory::slice_from_parts(pointer, 16);
                            switch (os::read(file, buffer)) {
                                when .Ok(count) { if count != 3 { return 2; }; }
                                when .Err(e) { return 3; }
                            };
                            switch (os::read(file, buffer)) {
                                when .Ok(count) { if count != 0 { return 4; }; }
                                when .Err(e) { return 5; }
                            };
                        }
                        when .Err(e) { return 6; }
                    };
                }
                when .Err(e) { return 7; }
            };
            return 0;
        }
    "#
    .replace("FILE", file.to_str().unwrap());
    std::fs::write(&plan.files[0].path, source).unwrap();
    plan.standard_library = StandardLibrary::at(sysroot());
    fibc::driver::run_pipeline(&plan, fibc::driver::PipelineGoal::Binary).unwrap();
    let output = std::process::Command::new(&plan.output).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

// Enumerate the installed tree so new std modules participate in mocked sysroots.
// The page-allocator instrumentation uses raw Linux x86-64 syscalls (mincore).
#[cfg(all(feature = "llvm", target_os = "linux", target_arch = "x86_64"))]
fn page_allocator_sysroot(source: &Path, destination: &Path) {
    std::fs::create_dir_all(destination).unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let target = destination.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            page_allocator_sysroot(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[cfg(all(feature = "llvm", target_os = "linux", target_arch = "x86_64"))]
fn page_allocator_input(source: &str) -> (tempfile::TempDir, BuildPlan) {
    let (directory, mut plan) = input(source);
    let installed = directory.path().join("sysroot");
    page_allocator_sysroot(&sysroot(), &installed);
    let path = installed.join("std/os.fib");
    let mut os = std::fs::read_to_string(&path).unwrap();
    os.push_str(r#"
        pub fn test_address(pointer: *uint8) uint64 {
            return root::os_abi::pointer_to_word(pointer);
        }
        pub fn test_alignment(pointer: *uint8, alignment: usize) bool {
            return test_address(pointer) % (alignment as uint64) == 0;
        }
        pub fn test_unmapped(address: uint64, size: usize) bool {
            page := address - address % 4096;
            last := (address + (size as uint64) - 1) / 4096 * 4096;
            vector: [1]uint8 = [0];
            while (page <= last) {
                if root::os::syscall3(27, page, 4096, root::memory::slice_data(vector[..])) != -12 { return false; };
                page = page + 4096;
            };
            return true;
        }
    "#);
    std::fs::write(path, os).unwrap();
    plan.standard_library = StandardLibrary::at(installed);
    (directory, plan)
}

#[test]
fn page_allocator_analyzes_and_lowers() {
    // The checked-in program uses only the public installed std API, so it also
    // works in the editor and outside the instrumented test sysroot.
    let (_directory, plan) = input(include_str!("programs/page_allocator.fib"));
    fibc::driver::run_pipeline(&plan, fibc::driver::PipelineGoal::Ir).unwrap();
}

#[cfg(all(feature = "llvm", target_os = "linux", target_arch = "x86_64"))]
fn instrumented_page_allocator_program() -> String {
    let source = include_str!("programs/page_allocator.fib");
    let allocation = "bytes := memory::slice_from_parts(pointer, size);";
    let released = "return true;";
    assert_eq!(source.matches(allocation).count(), 1);
    assert_eq!(source.matches(released).count(), 1);
    source
        .replace(
            allocation,
            &format!(
                r#"
        // Preserve only the numeric address after free; never dereference it.
        address := std::os::test_address(pointer);
        if !std::os::test_alignment(pointer, align) {{ return false; }};
        {allocation}
    "#
            ),
        )
        .replace(released, "return std::os::test_unmapped(address, size);")
}

#[test]
#[cfg(all(feature = "llvm", target_os = "linux", target_arch = "x86_64"))]
fn page_allocator_executes_and_unmaps() {
    let (_directory, plan) = page_allocator_input(&instrumented_page_allocator_program());
    fibc::driver::run_pipeline(&plan, fibc::driver::PipelineGoal::Binary).unwrap();
    let output = std::process::Command::new(&plan.output).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
#[cfg(all(feature = "llvm", target_os = "linux", target_arch = "x86_64"))]
fn page_allocator_scripted_mapping_and_unmapping_failures() {
    let (directory, plan) = page_allocator_input(
        r#"
        comp m := std::memory;
        fn main() int32 {
            a := m::page_allocator();
            if !rejected(a, 0, 8, m::AllocError.NotEnoughMemory) { return 1; };
            if !rejected(a, 1, 3, m::AllocError.InvalidAlignment) { return 2; };
            if !rejected(a, 18446744073709551615, 8, m::AllocError.NotEnoughMemory) { return 3; };
            if std::os::test_calls() != 0 { return 4; };
            std::os::test_mode(12);
            if !rejected(a, 1, 8, m::AllocError.NotEnoughMemory) { return 5; };
            std::os::test_mode(22);
            if !rejected(a, 1, 8, m::AllocError.MapFailed) { return 6; };
            std::os::test_mode(0);
            switch (a.alloc(a.context, 8193, 65536)) {
                when .Err(e) { return 7; }
                when .Ok(p) {
                    std::os::test_mode(1);
                    switch (a.free(a.context, p)) {
                        when .Ok { return 8; }
                        when .Err(e) { if error_code(e) != 4 { return 9; }; }
                    };
                    p* = 73;
                    std::os::test_mode(0);
                    address := std::os::test_address(p);
                    switch (a.free(a.context, p)) {
                        when .Err(e) { return 10; }
                        when .Ok {}
                    };
                    if !std::os::test_unmapped(address, 8193) { return 11; };
                }
            };
            std::os::test_mode(1);
            switch (a.free(a.context, null)) {
                when .Err(e) { return 12; }
                when .Ok {}
            };
            return 0;
        }
    "#,
    );
    let main = directory.path().join("main.fib");
    let helpers = include_str!("programs/page_allocator.fib")
        .split("fn exercise(")
        .next()
        .unwrap();
    std::fs::write(
        &main,
        format!("{helpers}\n{}", std::fs::read_to_string(&main).unwrap()),
    )
    .unwrap();
    let path = directory.path().join("sysroot/std/os.fib");
    let mut os = std::fs::read_to_string(&path).unwrap();
    os = os
        .replace("pub fn mmap(", "fn real_mmap(")
        .replace("pub fn munmap(", "fn real_munmap(");
    os.push_str(r#"
        mode: int32 = 0;
        calls: usize = 0;
        pub fn test_mode(value: int32) { mode = value; }
        pub fn test_calls() usize { return calls; }
        pub fn mmap(address: *uint8, length: usize, protection: int32, flags: int32, fd: int32, offset: int64) MappingResult {
            calls = calls + 1;
            if mode != 0 { return MappingResult.Err(mode); };
            return real_mmap(address, length, protection, flags, fd, offset);
        }
        pub fn munmap(address: *uint8, length: usize) Status {
            if mode != 0 { return Status.Err(22); };
            return real_munmap(address, length);
        }
    "#);
    std::fs::write(path, os).unwrap();
    fibc::driver::run_pipeline(&plan, fibc::driver::PipelineGoal::Binary).unwrap();
    let output = std::process::Command::new(&plan.output).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}
