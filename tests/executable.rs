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
    std::fs::write(
        &source,
        "fn main() int32 { x := 0; defer x = 1; return 42; }",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_fibc"))
        .args(["build", "--single-file"])
        .arg(&source)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unsupported statement"));
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

fn scalar_program(text: &str) -> std::process::ExitStatus {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("main.fib");
    std::fs::write(&source, text).unwrap();
    let plan = BuildPlan::single_file(&source).unwrap();
    run_pipeline(&plan, PipelineGoal::Binary).unwrap_or_else(|e| panic!("{text}\n{e:#}"));
    Command::new(&plan.output).status().unwrap()
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
    let plan = BuildPlan::single_file(&source).unwrap();
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
    std::fs::write(&build_file, include_str!("../example-project/build.fib")).unwrap();
    std::fs::write(
        directory.path().join("main.fib"),
        include_str!("../example-project/main.fib"),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_fibc"))
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
            "fn main() int32 { comp S: type = struct { x: int32 }; s := S { x: 42 }; comp deref := memory::unchecked_deref; p := s&; return deref(p).x; }",
            42,
        ),
        (
            "fn main() int32 { x := 42; p := x&; q := memory::unchecked_deref(p)&; if q == p { return q*; }; return 1; }",
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
            memory::unchecked_deref(target(trace&, s&)).x = mark(trace&, 4);
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
