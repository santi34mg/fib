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
            .env(
                "FIB_SYSROOT",
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("library"),
            )
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
    let mut plan = BuildPlan::single_file(&source)
        .unwrap()
        .with_standard_library(fibc::standard_library::StandardLibrary::at(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("library"),
        ));
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
fn out_of_range_character_exits_unsuccessfully_without_binary() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("main.fib");
    std::fs::write(
        &source,
        "fn main() int32 { defer { x := 'Ā'; }; return 42; }",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_fibc"))
        .env(
            "FIB_SYSROOT",
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("library"),
        )
        .args(["build", "--single-file"])
        .arg(&source)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("out of range for uint8"));
    assert!(!source.with_extension("").exists());
}

#[test]
fn linker_failure_preserves_existing_output() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("main.fib");
    std::fs::write(&source, include_str!("programs/return_42.fib")).unwrap();
    let plan = BuildPlan::single_file(&source)
        .unwrap()
        .with_standard_library(fibc::standard_library::StandardLibrary::at(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("library"),
        ));
    std::fs::write(&plan.output, b"existing output").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_fibc"))
        .env(
            "FIB_SYSROOT",
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("library"),
        )
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
            .env(
                "FIB_SYSROOT",
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("library"),
            )
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

fn scalar_program(text: &str) -> std::process::ExitStatus {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("main.fib");
    std::fs::write(&source, text).unwrap();
    let plan = BuildPlan::single_file(&source)
        .unwrap()
        .with_standard_library(fibc::standard_library::StandardLibrary::at(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("library"),
        ));
    run_pipeline(&plan, PipelineGoal::Binary).unwrap_or_else(|e| panic!("{text}\n{e:#}"));
    Command::new(&plan.output).status().unwrap()
}

#[test]
fn print_accepts_string_literals_without_slicing() {
    assert_eq!(scalar_program("fn main() int32 { switch (std::io::print(\"\")) { when .Ok { return '*' as int32; } when .Err { return 1; } }; }").code(), Some(42));
}

#[test]
fn checked_array_and_slice_operations_execute() {
    for text in [
        include_str!("programs/readonly_array_conversion.fib"),
        include_str!("../samples/const_bindings.fib"),
        "fn pick(s:const uint8[]) int32 { return s[0] as int32; } fn main() int32 { return pick(\"*\"); }",
        "s:const uint8[] = \"*\"; fn bytes() const uint8[] { return \"*\"; } fn main() int32 { x:const uint8[] = \"é\"; if x.len != 2 { return 1; }; x = bytes(); if x[0] != '*' { return 2; }; return s[0] as int32; }",
        include_str!("../samples/indexed_globals.fib"),
        "fn main() int32 { a := [1, 2, 3]; i := 1; a[i] = 42; return a[i]; }",
        "fn main() int32 { const a := [42, 2]; i := 0; return a[i]; }",
        "fn main() int32 { a := [1, 2, 3]; s := a[1..]; s[0] = 42; return a[1]; }",
        "fn main() int32 { a := [1, 2, 3]; s := a[..]; t := s[1.=1]; t[0] = 42; if t.len != 1 { return 1; }; return a[1]; }",
        "fn main() int32 { a := [42]; s := a[1..1]; t := a[..0]; if s.len == 0 && t.len == 0 { return a[0]; }; return 1; }",
        "fn main() int32 { a := [42]; i: uint128 = 0; return a[i]; }",
        "fn make() int32[2] { return [1, 42]; } fn main() int32 { i := 1; return make()[i]; }",
        "fn make() int32[2] { return [1, 42]; } fn main() int32 { s := make()[..]; return s[1]; }",
        "fn main() int32 { a := [[1, 2], [3, 4]]; i := 1; a[i][0] = 42; return a[1][0]; }",
        "comp P := struct { x: int32 }; fn main() int32 { a := [P { x: 1 }]; i := 0; a[i].x = 42; return a[0].x; }",
        "fn main() int32 { a := [1]; i := 0; p := a[i]&; p* = 42; return a[0]; }",
        "fn pick(a: int32[], i: int32) int32 { return a[i]; } fn main() int32 { a := [1, 42]; return pick(a[..], 1); }",
        "fn main() int32 { a: int32[0] = []; s := a[..]; if s.len == 0 { return 42; }; return 1; }",
        "fn main() int32 { a: int32[2]; a[0] = 1; a[1] = 42; i := 1; return a[i]; }",
        "fn main() int32 { s := \"*\"[..]; return s[0] as int32; }",
        "fn main() int32 { a := [1, 2]; b := a; b[0] = 42; if a[0] != 1 { return 1; }; return b[0]; }",
        "fn main() int32 { p: *int32 = null; s := std::memory::slice_from_parts(p, 0); empty := s[..]; if empty.len == 0 { return 42; }; return 1; }",
    ] {
        assert_eq!(data_program(text).code(), Some(42), "{text}");
    }
}

#[test]
fn checked_index_and_slice_failures_trap() {
    for text in [
        "fn main() int32 { a := [1]; i := -1; return a[i]; }",
        "fn main() int32 { a := [1]; i := 1; return a[i]; }",
        "fn main() int32 { a := [1]; i: uint128 = 18446744073709551616; return a[i]; }",
        "fn main() int32 { a := [1]; i: int128 = -18446744073709551616; return a[i]; }",
        "fn main() int32 { a: int32[0] = []; i := 0; a[i] = 42; return 1; }",
        "fn main() int32 { a := [1]; s := a[..]; i := 1; return s[i]; }",
        "fn main() int32 { a := [1]; i := -1; s := a[i..]; return s.len as int32; }",
        "fn main() int32 { a := [1]; i := 2; s := a[..i]; return s.len as int32; }",
        "fn main() int32 { a := [1, 2]; i := 2; j := 1; s := a[i..j]; return s.len as int32; }",
        "fn main() int32 { a := [1]; i: uint128 = 340282366920938463463374607431768211455; s := a[..i]; return s.len as int32; }",
        "fn main() int32 { a := [1]; i: uint64 = 18446744073709551615; s := a[0.=i]; return s.len as int32; }",
        "fn main() int32 { p: *int32 = null; s := std::memory::slice_from_parts(p, 1); return s[0]; }",
    ] {
        assert!(!data_program(text).success(), "{text}");
    }
}

#[test]
fn global_constant_data_and_relocations_execute() {
    for text in [
        "value := 41; fn main() int32 { value = value + 1; return value; }",
        "const value := 6 * 7; fn main() int32 { return value; }",
        "comp value := 6; answer := value * 7; fn main() int32 { return answer; }",
        "flag := (2.0 < 3.0) && (0.0 / 0.0 != 0.0); fn main() int32 { if flag { return 42; }; return 1; }",
        "table := [1, 2]; view := table[1..]; fn main() int32 { view[0] = 42; return table[1]; }",
        "view := table[1.=1]; table := [1, 2]; fn main() int32 { view[0] = 42; return table[1]; }",
        "view := [1, 42][..]; fn main() int32 { return view[1]; }",
        "comp view := [1, 2][..]; first := view; second := view; fn main() int32 { first[0] = 42; return second[0]; }",
        "first := [1][..]; second := [1][..]; fn main() int32 { first[0] = 42; if second[0] != 1 { return 1; }; return first[0]; }",
        "view := \"*\"[..]; fn main() int32 { return view[0] as int32; }",
        "comp P := struct { x: int32, y: int32 }; p := P { y: 1, x: 42 }; address := p.x&; fn main() int32 { return address*; }",
        "address := table[1]&; table := [1, 42]; fn main() int32 { return address*; }",
        "comp E := enum { Empty, Number: int32 }; value := E.Number(42); fn main() int32 { switch(value) { when .Empty { return 1; } when .Number(n) { return n; } }; }",
        "comp E := enum { Empty, Pointer: *int32 }; number := 42; value := E.Pointer(number&); fn main() int32 { switch(value) { when .Empty { return 1; } when .Pointer(p) { return p*; } }; }",
        "comp E := enum { Empty, Number: int32 }; values := [E.Empty, E.Number(42)]; fn main() int32 { switch(values[1]) { when .Empty { return 1; } when .Number(n) { return n; } }; }",
        "comp P := struct { next: *P, value: int32 }; first: P = P { next: second&, value: 1 }; second: P = P { next: first&, value: 42 }; fn main() int32 { return first.next.value; }",
        "first, const second := 40, 2; fn main() int32 { return first + second; }",
        "comp fn calculate() int32 { return 42; } answer := 42; fn main() int32 { return answer; }",
        "fn calculate() int32 { return 42; } callback := calculate; fn main() int32 { return callback(); }",
        "number := 42; pointer := (number&) as *void; fn main() int32 { p := pointer as *int32; return p*; }",
        "comp Pair := struct { first: int32, second: int32 }; comp E := enum { Nothing, Pair: Pair }; value := E.Pair(Pair { first: 40, second: 2 }); fn main() int32 { switch(value) { when .Nothing { return 1; } when .Pair(p) { return p.first + p.second; } }; }",
        "comp i := 1; table := [1, 42]; view := table[i..]; fn main() int32 { return view[0]; }",
        "fn main() int32 { comp value := 6 * 7; return value; }",
        "comp S := struct { label: int32, callback: fn(int32) -> int32 }; fn first(x: int32) int32 { return x + 1; } fn second(x: int32) int32 { return x + 2; } callbacks := [S { label: 1, callback: first }]; fn main() int32 { callbacks[0].callback = second; return callbacks[0].callback(40); }",
        "comp E := enum { Small: uint8, Wide: uint128, Pointer: *int32 }; comp S := struct { before: uint8, value: E, after: int32 }; number := 42; value := S { before: 1, value: E.Pointer(number&), after: 7 }; fn main() int32 { if value.after != 7 { return 1; }; switch(value.value) { when .Small(n) { return n as int32; } when .Wide(n) { return n as int32; } when .Pointer(p) { return p*; } }; }",
        "comp E := enum { Small: uint8, Wide: uint128, Pointer: *int32 }; number := 42; values := [E.Small(1), E.Pointer(number&), E.Wide(2)]; fn main() int32 { switch(values[1]) { when .Small(n) { return n as int32; } when .Wide(n) { return n as int32; } when .Pointer(p) { return p*; } }; }",
    ] {
        assert_eq!(data_program(text).code(), Some(42), "{text}");
    }
}

#[test]
fn indexed_assignment_and_slice_bounds_evaluate_once_in_order() {
    let text = r#"
        trace := 0;
        table := [1, 2];
        fn rhs() int32 { trace = trace * 10 + 1; return 42; }
        fn receiver() int32[] { trace = trace * 10 + 2; return table[..]; }
        fn index() int32 { trace = trace * 10 + 3; return 1; }
        fn start() int32 { trace = trace * 10 + 4; return 0; }
        fn end() int32 { trace = trace * 10 + 5; return 2; }
        fn main() int32 {
            receiver()[index()] = rhs();
            if trace != 123 { return 1; };
            trace = 0;
            view := receiver()[start()..end()];
            if trace != 245 { return 2; };
            return view[1];
        }
    "#;
    assert_eq!(data_program(text).code(), Some(42));
}

#[test]
fn multiple_files_share_global_storage_and_views() {
    let directory = tempfile::tempdir().unwrap();
    let main = directory.path().join("main.fib");
    let state = directory.path().join("state.fib");
    std::fs::write(&main, "comp storage := state; fn main() int32 { storage::update(); state::counter = state::view[1] + 1; return storage::counter; }").unwrap();
    std::fs::write(&state, "pub counter := 40; table := [1, 2]; pub view := table[..]; pub fn update() { counter = counter + 1; view[1] = counter; }").unwrap();
    let mut plan = BuildPlan::single_file(&main).unwrap();
    plan.standard_library = fibc::standard_library::StandardLibrary::disabled();
    plan.files.push(fibc::plan::SourceFile {
        id: fibc::plan::FileId(1),
        name: "state".to_owned().into(),
        path: state,
    });
    run_pipeline(&plan, PipelineGoal::Binary).unwrap();
    assert_eq!(
        Command::new(&plan.output).status().unwrap().code(),
        Some(42)
    );
}

/// Isolate storage semantics from source-library implementations. The minimal
/// source package still exercises normal installed compiler-intrinsic registration.
fn data_program(text: &str) -> std::process::ExitStatus {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("main.fib");
    std::fs::write(&source, text).unwrap();
    let mut plan = BuildPlan::single_file(&source).unwrap();
    plan.standard_library = fibc::standard_library::StandardLibrary::disabled();
    if text.contains("std::") {
        let root = directory.path().join("sysroot");
        std::fs::create_dir_all(root.join("std")).unwrap();
        std::fs::write(root.join("sysroot.toml"), format!("compiler_version = \"{}\"\ndeclaration_revision = {}\nsources = [\"memory.fib\", \"os.fib\", \"os_abi.fib\", \"prelude.fib\", \"result.fib\", \"map.fib\", \"process.fib\"]\n", env!("CARGO_PKG_VERSION"), fibc::standard_library::DECLARATION_REVISION)).unwrap();
        for name in [
            "memory", "os", "os_abi", "prelude", "result", "map", "process",
        ] {
            std::fs::write(root.join("std").join(format!("{name}.fib")), "").unwrap();
        }
        plan.standard_library = fibc::standard_library::StandardLibrary::at(root);
    }
    run_pipeline(&plan, PipelineGoal::Binary).unwrap_or_else(|e| panic!("{text}\n{e:#}"));
    Command::new(&plan.output).status().unwrap()
}

#[test]
fn sizeof_uses_target_storage_layout() {
    assert_eq!(
        data_program(include_str!("../samples/sizeof.fib")).code(),
        Some(42)
    );
    assert_eq!(
        data_program("fn main() int32 { return (core::sizeof(int32) as int32) + 38; }").code(),
        Some(42)
    );
}

#[test]
fn every_integer_width_wraps_and_crosses_calls_and_joins() {
    for width in [8, 16, 32, 64, 128] {
        for signed in [false, true] {
            let ty = format!("{}int{width}", if signed { "" } else { "u" });
            let max = if signed {
                (u128::MAX >> (129 - width)).to_string()
            } else {
                (u128::MAX >> (128 - width)).to_string()
            };
            let wrapped = if signed {
                format!("-{}", 1u128 << (width - 1))
            } else {
                "0".into()
            };
            let program = format!(
                r#"
                fn step(x: {ty}) {ty} {{ return x + 1; }}
                fn join(x: {ty}, flag: bool) {ty} {{
                    result := x;
                    if (flag) {{ result = step(x); }} else {{ result = x - 1; }};
                    return result;
                }}
                fn main() int32 {{
                    expected: {ty} = {max} + 1;
                    if (join({max}, true) == {wrapped}) {{
                        if (join({max}, true) == expected) {{ return 42; }};
                    }};
                    return 7;
                }}
            "#
            );
            assert_eq!(scalar_program(&program).code(), Some(42), "{ty}");
        }
    }
}

#[test]
fn integer_operators_and_casts_execute_at_every_width() {
    for width in [8, 16, 32, 64, 128] {
        for prefix in ["int", "uint"] {
            let ty = format!("{prefix}{width}");
            let program = format!(
                r#"
                fn calculate(a: {ty}, b: {ty}, count: uint128) bool {{
                    value := a;
                    value += b; value -= b; value *= b; value /= b; value %= b;
                    return a + b == 8 && a - b == 4 && a * b == 12 && a / b == 3
                        && a % b == 0 && (a & b) == 2 && (a | b) == 6 && (a ^ b) == 4
                        && (a << count) == 24 && (a >> count) == 1
                        && ~~a == a && -(-a) == a && value == 0;
                }}
                fn main() int32 {{ if (calculate(6, 2, 2)) {{ return 42; }}; return 7; }}
            "#
            );
            assert_eq!(scalar_program(&program).code(), Some(42), "{ty}");
        }
    }
    let casts = r#"
        fn casts(negative: int8, positive: uint8, wide: uint128) bool {
            return (negative as uint16) == 65535 && (negative as int128) == -1
                && (positive as int16) == 255 && (wide as uint8) == 255
                && (positive as int8) == -1 && (negative >> 1) == -1;
        }
        fn main() int32 { if (casts(-1, 255, 340282366920938463463374607431768211455)) { return 42; }; return 7; }
    "#;
    assert_eq!(scalar_program(casts).code(), Some(42));
}

#[test]
fn signed_division_overflow_wraps_and_remainder_is_zero() {
    for width in [8, 16, 32, 64, 128] {
        let min = format!("-{}", 1u128 << (width - 1));
        let text = format!(
            r#"
            fn divide(x: int{width}, y: int{width}) bool {{ return x / y == x && x % y == 0; }}
            fn main() int32 {{ if (divide({min}, -1)) {{ return 42; }}; return 7; }}
        "#
        );
        assert_eq!(scalar_program(&text).code(), Some(42));
    }
}

#[test]
fn floats_cross_calls_and_joins_with_ieee_arithmetic() {
    for width in [16, 32, 64, 128] {
        let text = format!(
            r#"
            fn calculate(a: float{width}, b: float{width}, flag: bool) float{width} {{
                value := a;
                if (flag) {{ value = a / b + a * b - b; }} else {{ value = -a; }};
                return value;
            }}
            fn ieee(zero: float{width}) bool {{
                nan := zero / zero;
                infinity := 1.0 / zero;
                negative := -zero;
                return nan != nan && !(nan == nan) && !(nan < zero) && !(nan >= zero)
                    && infinity > 1.0 && (1.0 / negative) < zero;
            }}
            fn convert(x: int128, y: float{width}) bool {{
                return (x as float{width}) == y && (y as uint128) == 13
                    && ((y as float128) as float{width}) == y;
            }}
            fn main() int32 {{
                if (calculate(6.0,2.0,true) == 13.0 && calculate(6.0,2.0,false) == -6.0
                    && ieee(0.0) && convert(13,13.0)) {{ return 42; }};
                return 7;
            }}
        "#
        );
        assert_eq!(scalar_program(&text).code(), Some(42), "float{width}");
    }
}

#[test]
fn short_circuiting_skips_calls_and_preserves_boolean_results() {
    let text = r#"
        fn fails(x: int32) bool { return 1 / x == 0; }
        fn choose(flag: bool, zero: int32) bool {
            return (flag || fails(zero)) && (!flag || (false && fails(zero)) == false);
        }
        fn main() int32 { if (choose(true,0)) { return 42; }; return 7; }
    "#;
    assert_eq!(scalar_program(text).code(), Some(42));
}

#[test]
fn invalid_runtime_numeric_operations_trap() {
    let programs = [
        "fn f(x: int32) int32 { return 1 / x; } fn main() int32 { return f(0); }",
        "fn f(x: uint128) int32 { return (1 as uint128) % x as int32; } fn main() int32 { return f(0); }",
        "fn f(x: int128) int32 { return 1 << x; } fn main() int32 { return f(-1); }",
        "fn f(x: uint128) int32 { return 1 >> x; } fn main() int32 { return f(340282366920938463463374607431768211455); }",
        "fn f(x: uint8) int32 { return (1 as int128) << x as int32; } fn main() int32 { return f(128); }",
        "fn f(x: float64) int32 { return x as int32; } fn main() int32 { return f(2147483648.0); }",
        "fn f(x: float128) int32 { return x as uint128 as int32; } fn main() int32 { return f(-1.0); }",
        "fn f(x: float16) int32 { return (x / x) as int32; } fn main() int32 { return f(0.0); }",
        "fn f(x: float32) int32 { return (1.0 / x) as int32; } fn main() int32 { return f(0.0); }",
    ];
    for text in programs {
        let status = scalar_program(text);
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            assert!(
                status.signal().is_some(),
                "expected a trap, got {status}: {text}"
            );
        }
        #[cfg(not(unix))]
        assert!(!status.success(), "{text}");
    }
}

#[test]
fn float_to_integer_checks_after_truncation() {
    let text = r#"
        fn f(x: float16, y: float128) bool { return (x as int8) == -128 && (y as uint8) == 0; }
        fn main() int32 { if (f(-128.5,-0.9)) { return 42; }; return 7; }
    "#;
    assert_eq!(scalar_program(text).code(), Some(42));
}

#[test]
fn exact_literals_and_runtime_float_rounding_agree() {
    let text = r#"
        fn half(x: float128) float16 { return x as float16; }
        fn single(x: float128) float32 { return x as float32; }
        fn integer(x: uint128) float128 { return x as float128; }
        fn main() int32 {
            a: float16 = 1.00048828125000000000000000000001;
            b: float32 = 1.000000059604644775390625000001;
            c: float128 = 9007199254740993.0;
            if (half(1.00048828125000000000000000000001) == a
                && a == 1.0009765625
                && single(1.000000059604644775390625000001) == b
                && b == 1.00000011920928955078125
                && integer(9007199254740993) == c) { return 42; };
            return 7;
        }
    "#;
    assert_eq!(scalar_program(text).code(), Some(42));
}

#[test]
fn builtin_scalar_sample_executes() {
    assert_eq!(
        scalar_program(include_str!("../samples/builtin_scalars.fib")).code(),
        Some(42)
    );
}

// Loop regressions must fail promptly if a backedge accidentally stops updating
// its carried values. Kill and reap the child before reporting a timeout.
fn bounded_program(text: &str) -> std::process::ExitStatus {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("main.fib");
    std::fs::write(&source, text).unwrap();
    let plan = BuildPlan::single_file(&source)
        .unwrap()
        .with_standard_library(fibc::standard_library::StandardLibrary::at(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("library"),
        ));
    run_pipeline(&plan, PipelineGoal::Binary).unwrap_or_else(|e| panic!("{text}\n{e:#}"));
    bounded_status(&plan.output)
}

fn bounded_status(binary: &std::path::Path) -> std::process::ExitStatus {
    let mut child = Command::new(binary).spawn().unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("program timed out: {}", binary.display());
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[test]
fn source_loops_breaks_continues_and_returns_execute() {
    for (text, expected) in [
        (include_str!("../samples/scalar_loops.fib"), 42),
        (
            "fn choose(flag: bool) int32 { x := 0; y := 0; while () { if (flag) { x = 40; y = 2; break; } else { x = 10; y = 11; break; }; }; return x + y; } fn main() int32 { return choose(true) + choose(false) - 21; }",
            42,
        ),
        (
            "fn main() int32 { x := 0; y := 0; while (x < 3) { inner := 0; while (inner < 3) { inner = inner + 1; if (inner < 3) { continue; }; y = y + 1; }; x = x + 1; continue; }; return y + 39; }",
            42,
        ),
        (
            "fn main() int32 { x := 42; while (false) { x = 7; }; return x; }",
            42,
        ),
        (
            "fn f(limit: int32) int32 { x := 0; while (x < limit) { x = x + 1; }; return x; } fn main() int32 { return f(0) + f(1) + f(41); }",
            42,
        ),
        (
            "fn main() int32 { x := 0; while (true) { x = x + 1; if (x < 42) { continue; }; break; }; return x; }",
            42,
        ),
        (
            "fn f(flag: bool) int32 { while (flag) { return 42; }; return 7; } fn main() int32 { return f(true); }",
            42,
        ),
        (
            "fn f(flag: bool) int32 { while (flag) { return 42; }; return 7; } fn main() int32 { return f(false); }",
            7,
        ),
        (
            "fn main() int32 { x := 0; while (x < 3 && 12 / (3 - x) > 0) { x = x + 1; continue; }; return x + 39; }",
            42,
        ),
        (
            "fn main() int32 { x := 0; while (x < 3 || 12 / (x - 2) > 0) { x = x + 1; if (x == 4) { break; }; }; return x + 38; }",
            42,
        ),
        (
            "fn main() int32 { x := 0; y := 0; while (x < 5) { x = x + 1; if (x == 3) { y = 42; break; } else { y = 7; }; }; return y; }",
            42,
        ),
        (
            "fn main() int32 { x := 0; while (x < 2) { y := 0; while (y < 2) { y = y + 1; }; x = x + y; }; return x + 40; }",
            42,
        ),
    ] {
        assert_eq!(bounded_program(text).code(), Some(expected), "{text}");
    }
}

#[test]
fn loops_carry_all_numeric_widths_and_bool() {
    for ty in [
        "int8", "int16", "int32", "int64", "int128", "uint8", "uint16", "uint32", "uint64",
        "uint128", "float16", "float32", "float64", "float128",
    ] {
        let text = format!(
            r#"
            fn advance(x: {ty}) {ty} {{ return x + 1; }}
            fn run(limit: {ty}) {ty} {{
                x: {ty} = 0;
                running := true;
                while (running) {{
                    x = advance(x);
                    if (x == limit) {{ running = false; }};
                }};
                return x;
            }}
            fn main() int32 {{ if (run(4) == 4) {{ return 42; }}; return 7; }}
        "#
        );
        assert_eq!(bounded_program(&text).code(), Some(42), "{ty}");
    }
}

#[test]
fn current_capabilities_project_builds_from_build_file_and_runs() {
    let directory = tempfile::tempdir().unwrap();
    let build_file = directory.path().join("build.fib");
    std::fs::write(&build_file, include_str!("programs/capabilities/build.fib")).unwrap();
    std::fs::write(
        directory.path().join("main.fib"),
        include_str!("programs/capabilities/main.fib"),
    )
    .unwrap();
    std::fs::write(
        directory.path().join("features.fib"),
        include_str!("programs/capabilities/features.fib"),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_fibc"))
        .env(
            "FIB_SYSROOT",
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("library"),
        )
        .args(["build", "--build-file"])
        .arg(&build_file)
        .current_dir(directory.path().parent().unwrap())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let binary = directory.path().join("demo");
    assert_eq!(bounded_status(&binary).code(), Some(0));
}

#[test]
fn structs_storage_and_pointer_aliases_execute() {
    for (text, expected) in [
        (include_str!("../samples/struct_storage.fib"), 42),
        (
            "fn main() int32 { x: int32; if true { x = 42; } else { x = 7; }; return x; }",
            42,
        ),
        (
            "fn main() int32 { x := 0; p := x&; while (x < 42) { p* = x + 1; }; return x; }",
            42,
        ),
        (
            "fn set(p: *int32) { p* = 42; } fn main() int32 { x := 0; p := x&; alias := p; set(alias); return x; }",
            42,
        ),
        (
            "fn change(p: **int32, value: *int32) { p* = value; } fn main() int32 { a := 7; b := 42; p := a&; change(p&, b&); return p*; }",
            42,
        ),
        (
            "comp S: type = struct { x: int32, y: int32 }; fn f(flag: bool) int32 { s: S; if flag { s.x = 42; } else { s.x = 7; }; return s.x; } fn main() int32 { return f(true); }",
            42,
        ),
        (
            "comp S: type = struct { x: int32 }; fn copy(s: S) S { s.x = 7; return s; } fn main() int32 { s := S { x: 42 }; t := copy(s); if t.x == 7 { return s.x; }; return 1; }",
            42,
        ),
        (
            "comp S: type = struct { x: int32 }; comp Alias := S; fn read(p: const *Alias) int32 { return (p*).x; } fn main() int32 { const s := S { x: 42 }; return read(s&); }",
            42,
        ),
        (
            "comp S: type = struct { x: int32 }; fn make(n: int32) S { if n == 0 { return S { x: 0 }; }; s := make(n - 1); s.x = s.x + 1; return s; } fn main() int32 { return make(42).x; }",
            42,
        ),
        (
            "comp S: type = struct { x: int32 }; fn main() int32 { s := S { x: 0 }; i := 0; while (i < 42) { i += 1; if i == 10 { continue; }; s.x = i; if i == 42 { break; }; }; return s.x; }",
            42,
        ),
        (
            "fn main() int32 { comp S: type = struct { x: int32 }; s := S { x: 42 }; comp deref := std::memory::unchecked_deref; p := s&; return deref(p).x; }",
            42,
        ),
        (
            "fn main() int32 { x := 42; p := x&; q := std::memory::unchecked_deref(p)&; if q == p { return q*; }; return 1; }",
            42,
        ),
    ] {
        assert_eq!(bounded_program(text).code(), Some(expected), "{text}");
    }
}

#[test]
fn struct_construction_and_assignment_preserve_evaluation_order() {
    let text = r#"
        comp S: type = struct { x: int32, y: int32 };
        fn mark(trace: *int32, digit: int32) int32 {
            trace* = (trace*) * 10 + digit;
            return digit;
        }
        fn target(trace: *int32, s: *S) *S {
            ignored := mark(trace, 3);
            return s;
        }
        fn main() int32 {
            trace := 0;
            s := S { y: mark(trace&, 1), x: mark(trace&, 2) };
            if trace != 12 || s.y != 1 || s.x != 2 { return 1; };
            std::memory::unchecked_deref(target(trace&, s&)).x = mark(trace&, 4);
            if trace != 1243 || s.x != 4 { return 2; };
            return 42;
        }
    "#;
    assert_eq!(bounded_program(text).code(), Some(42));
}

#[test]
fn ordinary_null_dereferences_trap_including_field_projection_and_address_of() {
    for text in [
        "fn read(p: *int32) int32 { return p*; } fn main() int32 { return read(null); }",
        "fn write(p: *int32) { p* = 42; } fn main() int32 { write(null); return 0; }",
        "comp S: type = struct { first: int32, second: int32 }; fn f(p: *S) int32 { return (p*).second; } fn main() int32 { return f(null); }",
        "comp S: type = struct { x: int32 }; fn f(p: *S) *int32 { return (p*).x&; } fn main() int32 { p := f(null); return 0; }",
    ] {
        let status = bounded_program(text);
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            assert!(status.signal().is_some(), "{text}: {status}");
        }
        #[cfg(not(unix))]
        assert!(!status.success(), "{text}: {status}");
    }
}

#[test]
fn structs_preserve_mixed_layout_and_every_numeric_field_width_across_calls() {
    for ty in [
        "int8", "int16", "int32", "int64", "int128", "uint8", "uint16", "uint32", "uint64",
        "uint128", "float16", "float32", "float64", "float128",
    ] {
        let literal = if ty.starts_with("float") {
            "42.0"
        } else {
            "42"
        };
        let text = format!(
            r#"
            comp Packet: type = struct {{ tag: uint8, value: {ty}, active: bool }};
            fn change(packet: Packet) Packet {{ packet.value = packet.value + 1; return packet; }}
            fn main() int32 {{
                original := Packet {{ active: true, value: {literal}, tag: 9 }};
                changed := change(original);
                if original.value == {literal} && changed.value == 43 && changed.tag == 9 && changed.active {{ return 42; }};
                return 7;
            }}
        "#
        );
        assert_eq!(bounded_program(&text).code(), Some(42), "{ty}");
    }
}

#[test]
fn enums_construct_switch_copy_and_cross_helper_boundaries() {
    for text in [
        include_str!("../samples/enum_runtime.fib"),
        "comp E: type = enum {}; fn impossible(e: E) int32 { switch(e) {}; } fn main() int32 { return 42; }",
        "comp E: type = enum { A: int32, B: int32 }; fn main() int32 { switch(E.A(42)) { when .A(v) { return v; } when .B(v) { return v; } }; }",
        "comp Large := struct { a: uint64, b: uint64, c: uint64, d: uint64, e: uint64 }; comp E: type = enum { Large: Large, Aligned: int128 }; fn copy(e: E) E { return e; } fn main() int32 { large := copy(E.Large(Large { a: 1, b: 2, c: 3, d: 4, e: 42 })); aligned := copy(E.Aligned(42)); switch(large) { when .Large(v) { if (v.a != 1 || v.b != 2 || v.c != 3 || v.d != 4 || v.e != 42) { return 7; }; } when else { return 1; } }; switch(aligned) { when .Aligned(v) { return v as int32; } when else { return 7; } }; }",
        "comp E: type = enum { A, B }; fn main() int32 { x := 7; switch(E.B) { when .A { x = 0; } when .B { x = 42; } }; return x; }",
        "comp E: type = enum { A, B }; fn main() int32 { x := 42; switch(E.B) { when .A { x = 7; } when else {} }; return x; }",
        "comp E: type = enum { N: int32, End }; fn main() int32 { e: E; p := e&; p* = E.N(42); switch(p*) { when .N(v) { return v; } when else { return 7; } }; }",
        "comp E: type = enum { N: int32, End }; fn main() int32 { x := 0; e := E.N(0); while (x < 42) { x += 1; e = E.N(x); switch(e) { when .N(v) { if v < 42 { continue; }; break; } when else { return 7; } }; }; switch(e) { when .N(v) { return v; } when else { return 7; } }; }",
        "comp E: type = enum { N: int32, End }; comp S: type = struct { e: E }; comp Outer: type = enum { Wrapped: S, Empty }; fn copy(e: Outer) Outer { return e; } fn main() int32 { e := copy(Outer.Wrapped(S { e: E.N(42) })); switch(e) { when .Wrapped(v) { switch(v.e) { when .N(n) { return n; } when else {} }; } when else {} }; return 7; }",
        "comp E: type = enum { Ref: *int32, Empty }; fn copy(e: E) E { return e; } fn main() int32 { x := 7; e := copy(E.Ref(x&)); switch(e) { when .Ref(v) { v* = 42; } when else {} }; return x; }",
        "comp E: type = enum { Next: *E, End }; fn main() int32 { e := E.End; link := E.Next(e&); switch(link) { when .Next(v) { switch(v*) { when .End { return 42; } when else {} }; } when else {} }; return 7; }",
    ] {
        assert_eq!(bounded_program(text).code(), Some(42), "{text}");
    }
}

#[test]
fn direct_enum_payloads_and_normalized_unit_values_execute() {
    assert_eq!(
        bounded_program(include_str!("../samples/enum_payloads.fib")).code(),
        Some(42)
    );
    let text = r#"
        comp Result := std::result::Result;
        fn ignore(T: type, value: T) {}
        fn inspect(T: type) int32 {
            comp R := Result(T, int32);
            value := R.Err(42);
            switch (value) {
                when .Ok(payload) { ignore(T, payload); return 0; }
                when .Err(error) { return error; }
            };
        }
        fn main() int32 {
            if inspect(int32) != 42 { return 1; };
            return inspect(void);
        }
    "#;
    assert_eq!(bounded_program(text).code(), Some(42));
}

#[test]
fn enum_payloads_preserve_target_alignment_and_numeric_widths() {
    for ty in [
        "int8", "uint16", "int32", "uint64", "int128", "uint128", "float16", "float32", "float64",
        "float128",
    ] {
        let text = format!(
            r#"
            comp Wide := struct {{ flag: bool, x: {ty}, tail: uint8 }};
            comp E: type = enum {{ Small: uint8, Wide: Wide }};
            fn copy(e: E) E {{ return e; }}
            fn main() int32 {{
                e := copy(E.Wide(Wide {{ flag: true, x: 42, tail: 7 }}));
                switch(e) {{
                    when .Wide(v) {{ if (v.flag && v.x == 42 && v.tail == 7) {{ return (v.x as int32); }}; }}
                    when .Small(v) {{ return (v as int32); }}
                }};
                return 1;
            }}
        "#
        );
        assert_eq!(bounded_program(&text).code(), Some(42), "{text}");
    }
}

#[test]
fn enum_subjects_and_constructor_arguments_evaluate_once_in_order() {
    let text = r#"
        comp Pair := struct { first: int32, second: int32 };
        comp E: type = enum { Pair: Pair, Empty };
        fn mark(p: *int32, digit: int32) int32 { p* = (p*) * 10 + digit; return digit; }
        fn make(p: *int32) E { return E.Pair(Pair { first: mark(p, 1), second: mark(p, 2) }); }
        fn main() int32 {
            trace := 0;
            switch(make(trace&)) {
                when .Pair(v) { if (trace == 12 && v.first == 1 && v.second == 2) { return 42; }; }
                when else {}
            };
            return 7;
        }
    "#;
    assert_eq!(bounded_program(text).code(), Some(42));
}

#[test]
fn specification_conversions_views_and_generic_instances_execute() {
    for text in [
        "fn f(x:float64) bool { return x as bool; } fn g(x:bool) float16 { return x as float16; } fn main() int32 { z := -0.0; nan := z/z; if(f(z)) { return 1; }; if(!f(nan)) { return 2; }; if(g(true) != 1.0) { return 3; }; n := 300; if(n as uint8 != 44) { return 4; }; wide := 4294967296 as uint64; return (wide-4294967254) as int32; }",
        "comp S:type = struct { x:int32 }; fn read(p:const *S) int32 { return p.x; } fn main() int32 { s := S { x:1 }; p := s&; p.x = 42; return read(p); }",
        "fn id(T:type, x:T) T { return x; } fn main() int32 { a := id(int32, 20); b := id(int64, 22); return id(int32, a+(b as int32)); }",
        "comp S:type = struct { x:int32 }; fn id(T:type, x:T) T { return x; } fn main() int32 { s := id(S, S { x:42 }); return s.x; }",
        "fn sum(T:type, x:T) T { if(x == 0) { return 0; }; return x+sum(T,x-1); } fn main() int32 { return sum(int32, 6)+21; }",
        "fn make(p:*int32) int32[2] { (p*) = 40; return [1,2]; } fn id(T:type, x:T) T { return x; } fn main() int32 { n := 0; a := id(int32[2], make(n&)); b := a; b = [3,4]; return n+(b.len as int32); }",
        "comp A:type = int32[0]; fn empty() A { return []; } fn main() int32 { const a := empty(); return (a.len as int32)+42; }",
        "comp E:type = enum { A: int32[2] }; fn id(e:E) E { return e; } fn main() int32 { e := id(E.A([1,2])); switch(e) { when .A(x) { return (x.len as int32)+40; } }; }",
        "fn set(p:*int32, n:int32) int32 { (p*) = n; return n; } fn main() int32 { outer := 0; const a:int32, b:int32 = set(outer&,20), set(outer&,22); return a+b; }",
        "comp S:type = struct { x:int32 }; fn lhs(p:*int32,s:*S) *S { (p*) = (p*)*10+2; return s; } fn rhs(p:*int32) int32 { (p*) = (p*)*10+1; return 42; } fn main() int32 { trace := 0; s := S { x:0 }; lhs(trace&,s&).x = rhs(trace&); if(trace != 12) { return 1; }; return s.x; }",
        "fn accumulate(T:type,n:T) T { x:T = 0; i:T = 0; while(i<n) { x = x+i; i = i+1; }; return x; } fn main() int32 { return (accumulate(int64,7) as int32)+21; }",
        "comp S:type = struct { x:int32 }; fn field(T:type, value:T) int32 { return value.x; } fn main() int32 { s := S { x:21 }; return field(S,s)+field(*S,s&); }",
        include_str!("../samples/specification_runtime.fib"),
        "fn size(p:*int32[2]) int32 { return p.len as int32; } fn main() int32 { a := [1,2]; return size(a&)+40; }",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("main.fib");
        std::fs::write(&source, text).unwrap();
        let plan = BuildPlan::single_file(&source)
            .unwrap()
            .with_standard_library(fibc::standard_library::StandardLibrary::at(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("library"),
            ));
        run_pipeline(&plan, PipelineGoal::Binary).unwrap_or_else(|e| panic!("{text}\n{e:#}"));
        assert_eq!(
            Command::new(&plan.output).status().unwrap().code(),
            Some(42),
            "{text}"
        );
    }
}

#[test]
fn multiple_files_preserve_entry_and_declaration_identities() {
    use fibc::plan::{FileId, PackageId, PackageSpec, SourceFile};
    let directory = tempfile::tempdir().unwrap();
    let main = directory.path().join("main.fib");
    let helper = directory.path().join("helper.fib");
    std::fs::write(
        &main,
        "fn same() int32 { return 20; } fn main() int32 { return same()+helper::same(); }",
    )
    .unwrap();
    std::fs::write(
        &helper,
        "pub(root) fn same() int32 { return 22; } fn main() int32 { return 7; }",
    )
    .unwrap();
    let mut plan = BuildPlan::single_file(&main)
        .unwrap()
        .with_standard_library(fibc::standard_library::StandardLibrary::at(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("library"),
        ));
    plan.files.push(SourceFile {
        id: FileId(1),
        name: "helper".to_string().into(),
        path: helper,
    });
    plan.packages = vec![PackageSpec {
        id: PackageId(42),
        source_root: directory.path().to_owned(),
        dependencies: Default::default(),
        files: vec![FileId(0), FileId(1)],
    }];
    run_pipeline(&plan, PipelineGoal::Binary).unwrap();
    assert_eq!(
        Command::new(&plan.output).status().unwrap().code(),
        Some(42)
    );
}

#[test]
fn boolean_casts_cover_every_numeric_runtime_representation() {
    let mut text = String::new();
    let mut checks = String::new();
    for ty in [
        "int8", "int16", "int32", "int64", "int128", "uint8", "uint16", "uint32", "uint64",
        "uint128", "float16", "float32", "float64", "float128",
    ] {
        text.push_str(&format!("fn to_{ty}(x:{ty}) bool {{ return x as bool; }} fn from_{ty}(b:bool) {ty} {{ return b as {ty}; }}"));
        let (zero, one) = if ty.starts_with("float") {
            ("0.0", "1.0")
        } else {
            ("0", "1")
        };
        checks.push_str(&format!("if(to_{ty}({zero}) || !to_{ty}({one}) || from_{ty}(true) != {one} || from_{ty}(false) != {zero}) {{ return 1; }};"));
    }
    text.push_str(&format!("fn main() int32 {{ {checks} return 42; }}"));
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("main.fib");
    std::fs::write(&source, text).unwrap();
    let plan = BuildPlan::single_file(&source)
        .unwrap()
        .with_standard_library(fibc::standard_library::StandardLibrary::at(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("library"),
        ));
    run_pipeline(&plan, PipelineGoal::Binary).unwrap();
    assert_eq!(
        Command::new(&plan.output).status().unwrap().code(),
        Some(42)
    );
}

#[test]
fn implicit_pointer_access_warns_and_traps_on_null() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("main.fib");
    std::fs::write(&source, "comp S:type = struct { x:int32 }; fn read(p:*S) int32 { return p.x; } fn main() int32 { return read(null); }").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_fibc"))
        .env(
            "FIB_SYSROOT",
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("library"),
        )
        .args(["build", "--single-file"])
        .arg(&source)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let warnings = String::from_utf8_lossy(&output.stderr);
    assert_eq!(warnings.matches("warning[type]").count(), 1);
    assert!(warnings.contains("main.fib:1:"));
    let plan = BuildPlan::single_file(&source)
        .unwrap()
        .with_standard_library(fibc::standard_library::StandardLibrary::at(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("library"),
        ));
    assert!(!Command::new(&plan.output).status().unwrap().success());
}

#[test]
fn defer_blocks_execute_on_lexical_exits() {
    for text in [
        // Reverse order, with values read at exit rather than registration.
        "fn main() int32 { x := 0; if true { defer { x = x * 10 + 2; }; defer x = x * 10 + 4; }; return x; }",
        // Deferred locals and branches have their own scope; empty blocks work.
        "fn choose(b: bool) int32 { x := 1; if b { defer { temporary := 40; if b { temporary = temporary + 2; }; }; defer {}; } else { defer x = 9; }; return x; } fn main() int32 { return choose(true) + choose(false) + 32; }",
        // Normal loop exit and continue each run exactly one body cleanup.
        "fn f(n: int32) int32 { x := 0; i := 0; while (i < n) { defer { x = x + 7; }; i = i + 1; if i < n { continue; }; }; return x; } fn main() int32 { return f(6); }",
        // Break leaves inner scopes, preserving the enclosing function defer.
        "fn main() int32 { x := 0; defer x = 99; while (true) { defer x = x + 2; if true { defer x = x + 40; break; }; }; return x; }",
        // Return captures the result before all pending cleanups, inner first.
        "fn f(p: *int32, b: bool) int32 { defer { p* = (p*) * 10 + 2; }; if b { defer p* = (p*) * 10 + 4; return p*; }; return 9; } fn main() int32 { x := 0; r := f(x&, true); return x + r; }",
        // Distinct dynamic return paths retain their own cleanup registrations.
        "fn f(p: *int32, b: bool) { defer p* = (p*) + 20; if b { defer p* = (p*) + 1; return; }; defer p* = (p*) + 1; } fn main() int32 { x := 0; f(x&, true); f(x&, false); return x; }",
        // Return unwinds loop scopes and function scopes.
        "fn f(p: *int32) { defer p* = (p*) + 2; while (true) { defer p* = (p*) + 40; return; }; } fn main() int32 { x := 0; f(x&); return x; }",
        // A nested loop's break must leave the outer body cleanup registered.
        "fn main() int32 { x := 0; i := 0; while (i < 2) { defer x = x + 20; while (true) { defer x = x + 1; break; }; i = i + 1; }; return x; }",
        // Storage classification visits address-taking and aggregates in cleanup.
        "comp S: type = struct { x: int32 }; fn set(p: *int32) { p* = 42; } fn main() int32 { x := 0; if true { defer { s := S { x: 1 }; set(s.x&); x = s.x; }; }; return x; }",
        // Cleanup can initialize a previously uninitialized outer binding.
        "fn main() int32 { x: int32; if true { defer { x = 42; }; }; return x; }",
        // Switch arm cleanup runs before the enclosing body cleanup.
        "comp E: type = enum { A, B }; fn f(e: E) int32 { x := 0; switch(e) { when .A { defer x = 42; } when .B { defer x = 7; } }; return x; } fn main() int32 { return f(E.A); }",
        // Concrete generic instances also retain deferred bodies.
        "fn f(T: type, p: *T, x: T) { defer { p* = x; }; } fn main() int32 { x := 0; f(int32, x&, 42); return x; }",
        // Aggregate return values are copied before cleanup mutates storage.
        "comp S: type = struct { x: int32 }; fn f() S { s := S { x: 42 }; defer s.x = 7; return s; } fn main() int32 { return f().x; }",
    ] {
        assert_eq!(bounded_program(text).code(), Some(42), "{text}");
    }
    assert_eq!(
        bounded_program(include_str!("../samples/deferred_blocks.fib")).code(),
        Some(42)
    );
}

#[test]
fn function_pointer_struct_fields_construct_and_execute() {
    let cases: Vec<(&str, i32)> = vec![
        // Basic struct with function pointer field
        (
            r#"
            comp Callback: type = fn(int32) int32;
            comp S: type = struct { value: int32, callback: Callback };
            fn add_one(x: int32) int32 { return x + 1; }
            fn main() int32 {
                s := S { value: 42, callback: add_one };
                result := (s.callback)(s.value);
                if result == 43 { return 0; } else { return 1; };
            }
        "#,
            0,
        ),
        // Struct with multiple function pointers
        (
            r#"
            comp Callback: type = fn(int32) int32;
            comp S: type = struct { a: int32, b: int32, callback_a: Callback, callback_b: Callback };
            fn double(x: int32) int32 { return x * 2; }
            fn triple(x: int32) int32 { return x * 3; }
            fn main() int32 {
                s := S { a: 10, b: 20, callback_a: double, callback_b: triple };
                result_a := (s.callback_a)(s.a);
                result_b := (s.callback_b)(s.b);
                if result_a == 20 && result_b == 60 { return 0; } else { return 1; };
            }
        "#,
            0,
        ),
        // Function pointer in struct with other fields
        (
            r#"
            comp Callback: type = fn(int32, int32) int32;
            comp S: type = struct { name: const uint8[], multiplier: int32, callback: Callback };
            fn multiply(x: int32, y: int32) int32 { return x * y; }
            fn main() int32 {
                s := S { name: "test"[..], multiplier: 5, callback: multiply };
                result := (s.callback)(4, s.multiplier);
                if result == 20 { return 0; } else { return 1; };
            }
        "#,
            0,
        ),
        // Function pointer that modifies struct field
        (
            r#"
            comp Callback: type = fn(*int32) void;
            comp S: type = struct { value: int32, callback: Callback };
            fn increment(p: *int32) {
                p* = (p*) + 10;
            }
            fn main() int32 {
                s := S { value: 5, callback: increment };
                s.callback(s.value&);
                if s.value == 15 { return 0; } else { return 1; };
            }
        "#,
            0,
        ),
        // Function pointer passed as argument to another function using struct field
        (
            r#"
            comp Callback: type = fn(int32) int32;
            comp S: type = struct { value: int32, callback: Callback };
            fn apply(callback: Callback, input: int32) int32 {
                return (callback)(input);
            }
            fn double(x: int32) int32 { return x * 2; }
            fn main() int32 {
                s := S { value: 100, callback: double };
                result := apply(s.callback, s.value / 2);
                if result == 100 { return 0; } else { return 1; };
            }
        "#,
            0,
        ),
        // Nested function pointers in struct
        (
            r#"
            comp Callback: type = fn(int32) int32;
            comp S: type = struct { value: int32, multiplier: int32, callback: Callback };
            fn add(x: int32) int32 { return x + 1; }
            fn main() int32 {
                s := S { value: 5, multiplier: 3, callback: add };
                result := (s.callback)(s.value * s.multiplier);
                if result == 16 { return 0; } else { return 1; };
            }
        "#,
            0,
        ),
    ];

    for (source_text, expected) in cases {
        let status = bounded_program(source_text);
        assert_eq!(status.code(), Some(expected), "Failed for:\n{source_text}");
    }
}

#[test]
fn function_pointer_callback_in_struct_with_multiple_fields() {
    // Test that function pointers work correctly in structs with many fields
    let source = r#"
        comp Callback: type = fn(int32) int32;
        comp S: type = struct {
            x: int32,
            y: int32,
            z: uint,
            callback: Callback,
            name: const uint8[]
        };
        fn square(x: int32) int32 { return x * x; }
        fn main() int32 {
            s := S {
                x: 7,
                y: 14,
                z: 3,
                callback: square,
                name: "test"[..]
            };
            result := (s.callback)(s.x + s.y);
            if result == 441 { return 0; } else { return 1; };
        }
    "#;

    let status = bounded_program(source);
    assert_eq!(
        status.code(),
        Some(0),
        "Function pointer in struct with multiple fields failed"
    );
}

#[test]
fn function_pointer_in_struct_can_be_assigned_and_called() {
    // Test assigning and calling function pointers stored in structs
    let source = r#"
        comp Callback: type = fn(int32) int32;
        comp S: type = struct { callback: Callback };
        fn add_one(x: int32) int32 { return x + 1; }
        fn sub_one(x: int32) int32 { return x - 1; }

        fn main() int32 {
            // Create multiple structs with different callbacks
            s1 := S { callback: add_one };
            s2 := S { callback: sub_one };

            result1 := (s1.callback)(10);
            result2 := (s2.callback)(10);

            if result1 == 11 && result2 == 9 { return 0; } else { return 1; };
        }
    "#;

    let status = bounded_program(source);
    assert_eq!(
        status.code(),
        Some(0),
        "Function pointer assignment and call failed"
    );
}

#[test]
fn struct_with_function_pointer_can_be_passed_as_argument() {
    // Test passing a struct containing a function pointer as an argument
    let source = r#"
        comp Callback: type = fn(int32) int32;
        comp S: type = struct { callback: Callback };
        comp Result: type = struct { input: int32, output: int32 };

        fn apply_callback(s: S, input: int32) Result {
            return Result { input: input, output: (s.callback)(input) };
        }

        fn double(x: int32) int32 { return x * 2; }

        fn main() int32 {
            s := S { callback: double };
            result := apply_callback(s, 21);

            if result.output == 42 && result.input == 21 { return 0; } else { return 1; };
        }
    "#;

    let status = bounded_program(source);
    assert_eq!(
        status.code(),
        Some(0),
        "Passing struct with function pointer as argument failed"
    );
}

#[test]
fn function_pointer_in_struct_with_array_fields() {
    // Test function pointers in structs that also contain arrays
    let source = r#"
        comp Callback: type = fn(int32) int32;
        comp S: type = struct {
            value: int32,
            callback: Callback,
            counts: uint[5]
        };
        fn increment(x: int32) int32 { return x + 1; }

        fn main() int32 {
            s := S {
                value: 42,
                callback: increment,
                counts: [1, 2, 3, 4, 5]
            };
            result := (s.callback)(s.value);
            if result == 43 && s.counts[0] == 1 { return 0; } else { return 1; };
        }
    "#;

    let status = bounded_program(source);
    assert_eq!(
        status.code(),
        Some(0),
        "Function pointer in struct with array fields failed"
    );
}

#[test]
fn function_pointer_in_struct_with_enum_fields() {
    // Test function pointers in structs that also contain enums
    let source = r#"
        comp Callback: type = fn(int32) int32;
        comp Status: type = enum { Success, Failure };

        comp S: type = struct {
            value: int32,
            callback: Callback,
            status: Status
        };
        fn square(x: int32) int32 { return x * x; }

        fn main() int32 {
            s := S {
                value: 7,
                callback: square,
                status: Status.Success
            };
            result := (s.callback)(s.value);
            if result != 49 { return 1; };
            switch(s.status) {
                when .Success { return 0; }
                when .Failure { return 1; }
            };
        }
    "#;

    let status = bounded_program(source);
    assert_eq!(
        status.code(),
        Some(0),
        "Function pointer in struct with enum fields failed"
    );
}

#[test]
fn function_pointer_mutations_in_struct() {
    // Test that function pointers stored in structs can be mutated
    let source = r#"
        comp Callback: type = fn(int32) int32;
        comp S: type = struct { callback: Callback, value: int32 };
        fn add(x: int32) int32 { return x + 1; }
        fn double(x: int32) int32 { return x * 2; }

        fn main() int32 {
            // Create a struct with an initial callback
            s := S { callback: add, value: 10 };

            // Mutate the function pointer field
            s.callback = double;

            result := (s.callback)(s.value);

            if result == 20 { return 0; } else { return 1; };
        }
    "#;

    let status = bounded_program(source);
    assert_eq!(
        status.code(),
        Some(0),
        "Function pointer mutation in struct failed"
    );
}

#[test]
fn function_pointer_in_struct_with_nested_structs() {
    // Test function pointers in structs with nested struct definitions
    let source = r#"
        comp Callback: type = fn(int32) int32;

        comp Inner: type = struct { value: int32 };
        comp Outer: type = struct {
            inner: Inner,
            callback: Callback
        };

        fn increment(x: int32) int32 { return x + 1; }

        fn main() int32 {
            o := Outer {
                inner: Inner { value: 42 },
                callback: increment
            };
            result := (o.callback)(o.inner.value);
            if result == 43 { return 0; } else { return 1; };
        }
    "#;

    let status = bounded_program(source);
    assert_eq!(
        status.code(),
        Some(0),
        "Function pointer in struct with nested structs failed"
    );
}
