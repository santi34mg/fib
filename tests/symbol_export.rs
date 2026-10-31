use std::process::{Command, Output};

fn export(source: &str) -> (tempfile::TempDir, Output) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("api.fib");
    std::fs::write(&path, source).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_fibc"))
        .args(["export-symbols", "--no-std", "--single-file"])
        .arg(path)
        .output()
        .unwrap();
    (directory, output)
}

fn document(output: &Output) -> serde_json::Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("stdout must contain only JSON")
}

#[test]
fn exports_public_functions_types_values_and_aliases_without_main() {
    let (directory, output) = export(
        "pub comp Pair := struct { x: int32 };\n\
         pub const limit := 4;\n\
         pub fn identity(T: type, value: T) T { return value; }\n\
         pub comp alias := identity;\n\
         pub comp Integer := int32;\n\
         pub first, pub const second := 1, 2;\n\
         pub fn nested() { comp Local := struct { n: int32 }; local := 1; }\n\
         fn hidden() {}\n\
         pub(dir) fn directory_only() {}\n\
         pub(root) fn package_only() {}",
    );
    let doc = document(&output);
    assert_eq!(doc["schema_version"], 1);
    let symbols = doc["symbols"].as_array().unwrap();
    let names: Vec<_> = symbols
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "Integer", "Pair", "alias", "first", "identity", "limit", "nested", "second"
        ]
    );
    assert_eq!(symbols[1]["kind"], "type");
    assert_eq!(symbols[2]["target"]["name"], "identity");
    assert_eq!(symbols[4]["signature"]["parameters"][1]["type"], "T");
    assert_eq!(symbols[4]["signature"]["return_type"], "T");
    assert_eq!(symbols[5]["type"], "int32");
    assert_eq!(symbols[5]["source"]["line"], 2);
    assert_eq!(symbols[5]["module"], "api");
    assert!(
        !directory.path().join("api").exists(),
        "export must not produce a binary"
    );
    let again = Command::new(env!("CARGO_BIN_EXE_fibc"))
        .args(["export-symbols", "--no-std", "--single-file"])
        .arg(directory.path().join("api.fib"))
        .output()
        .unwrap();
    assert_eq!(output.stdout, again.stdout);
}

#[test]
fn empty_public_surface_is_an_empty_array() {
    let (_directory, output) = export("fn private() {} const hidden := 1;");
    assert_eq!(document(&output)["symbols"], serde_json::json!([]));
}

#[test]
fn public_aliases_preserve_synthetic_compiler_targets() {
    let (_directory, output) = export("pub comp size := core::sizeof;");
    let doc = document(&output);
    assert_eq!(doc["symbols"][0]["target"]["package"], "compiler");
    assert_eq!(doc["symbols"][0]["target"]["module"], "core");
    assert_eq!(doc["symbols"][0]["target"]["name"], "sizeof");
}

#[test]
fn includes_selected_std_sources_but_not_the_implicit_prelude() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("api.fib");
    std::fs::write(&path, "pub const answer := 42;").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_fibc"))
        .args(["export-symbols", "--single-file"])
        .arg(path)
        .arg("--sysroot")
        .arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("library"))
        .output()
        .unwrap();
    let doc = document(&output);
    let symbols = doc["symbols"].as_array().unwrap();
    assert!(symbols.iter().any(|s| s["name"] == "answer"));
    assert!(symbols.iter().any(|s| s["name"] == "create_page_alloc"));
    assert!(symbols.iter().all(|s| s["package"] != "compiler"));
}

#[test]
fn invalid_sources_fail_without_partial_json() {
    for source in ["pub fn broken( {", "pub fn broken() { missing; }"] {
        let (_directory, output) = export(source);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
}

#[test]
fn build_file_exports_all_planned_files_and_namespace_aliases() {
    let directory = tempfile::tempdir().unwrap();
    for (name, source) in [
        (
            "build.fib",
            "fn build() int32 { build_api::add_source(\"api.fib\"); build_api::add_source(\"util.fib\"); build_api::set_entry(\"api\"); build_api::set_output(\"unused\"); return 0; }",
        ),
        (
            "api.fib",
            "pub comp Library := util; pub comp exposed := util::value;",
        ),
        ("util.fib", "pub const value := 3; const hidden := 9;"),
    ] {
        std::fs::write(directory.path().join(name), source).unwrap();
    }
    let output = Command::new(env!("CARGO_BIN_EXE_fibc"))
        .args(["export-symbols", "--no-std", "--build-file"])
        .arg(directory.path().join("build.fib"))
        .output()
        .unwrap();
    let doc = document(&output);
    let symbols = doc["symbols"].as_array().unwrap();
    assert_eq!(symbols.len(), 3);
    assert_eq!(symbols[0]["target"]["module"], "util");
    assert_eq!(symbols[1]["target"]["name"], "value");
    assert_eq!(symbols[2]["module"], "util");
    assert!(!directory.path().join("unused").exists());
}

#[test]
fn requires_exactly_one_input_mode() {
    for arguments in [
        vec!["export-symbols"],
        vec![
            "export-symbols",
            "--single-file",
            "a.fib",
            "--build-file",
            "build.fib",
        ],
        vec![
            "export-symbols",
            "--single-file",
            "a.fib",
            "--sysroot",
            "library",
            "--no-std",
        ],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_fibc"))
            .args(arguments)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
    }
}
