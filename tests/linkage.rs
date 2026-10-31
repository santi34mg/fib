//! Canonical linkage must survive changes to build-local IDs and checkout paths.
use fibc::{
    driver::{PipelineGoal, compiler_output::CompilationOutput, run_pipeline},
    ir::{FunctionBody, Ir},
    plan::{BuildPlan, FileId, PackageId, PackageSpec, SourceFile},
    standard_library::StandardLibrary,
};

fn fixture(root: &std::path::Path, alternate: bool) -> BuildPlan {
    let alias = if alternate { "another_name" } else { "shared" };
    let modules = [
        ("app/Main.fib", "comp l := dep_left::api; comp call := l::value; fn helper() int32 { return 20; } fn main() int32 { return helper()+call()+dep_right::api::value(); }".to_owned()),
        ("left/api.fib", format!("pub comp value := {alias}::values::value;")),
        ("right/api.fib", format!("pub comp value := {alias}::values::value;")),
        ("common/Values.fib", "pub stored: int32 = 11; fn helper() int32 { return stored; } fn identity(T:type, v:T) T { return v; } pub fn value() int32 { return identity(int32, helper()); }".into()),
    ];
    let file_id = |i: usize| FileId(if alternate { 100 - i as u32 } else { i as u32 });
    let package_id = |i: usize| PackageId(if alternate { 20 - i as u32 } else { i as u32 });
    let mut files = Vec::new();
    for (i, (path, source)) in modules.iter().enumerate() {
        let path = root.join(if alternate {
            path.replace("Main", "mAIN").replace("Values", "vALUES")
        } else {
            (*path).to_owned()
        });
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, source).unwrap();
        files.push(SourceFile {
            id: file_id(i),
            name: "unused".to_string().into(),
            path,
        });
    }
    let mut packages = Vec::new();
    for (i, name) in ["app", "left", "right", "common"].iter().enumerate() {
        packages.push(PackageSpec {
            id: package_id(i),
            identity: format!("test:{name}@1"),
            source_root: root.join(name),
            dependencies: match i {
                0 => [
                    ("dep_left".into(), package_id(1)),
                    ("dep_right".into(), package_id(2)),
                ]
                .into(),
                1 | 2 => [(alias.into(), package_id(3))].into(),
                _ => Default::default(),
            },
            files: vec![file_id(i)],
        });
    }
    if alternate {
        files.reverse();
        packages.reverse();
    }
    BuildPlan {
        standard_library: StandardLibrary::disabled(),
        native: Default::default(),
        packages,
        files,
        entry: file_id(0),
        output: root.join("demo"),
    }
}

fn lower(plan: &BuildPlan) -> Ir {
    let CompilationOutput::Ir(ir) = run_pipeline(plan, PipelineGoal::Ir).unwrap() else {
        panic!("IR output");
    };
    ir
}

fn symbols(ir: &Ir) -> Vec<String> {
    let mut symbols: Vec<_> = ir
        .functions
        .iter()
        .map(|f| f.identifier.clone())
        .chain(ir.globals.iter().map(|g| g.name.clone()))
        .collect();
    symbols.sort();
    symbols
}

#[test]
fn relocation_ids_aliases_and_diamond_edges_preserve_symbols() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let first = lower(&fixture(a.path(), false));
    let second = lower(&fixture(b.path(), true));
    assert_eq!(symbols(&first), symbols(&second));
    assert_eq!(first.functions.len(), 5); // entry, two helpers, generic instance, value
    assert!(
        first
            .functions
            .iter()
            .all(|f| matches!(f.body, FunctionBody::Definition { .. }))
    );
    let names = symbols(&first);
    assert_eq!(names.iter().filter(|n| n.ends_with("6_helper")).count(), 2);
    assert_eq!(
        names
            .iter()
            .filter(|n| n.starts_with("fib.v1.instance."))
            .count(),
        1
    );
    assert_eq!(names.iter().filter(|n| *n == "main").count(), 1);
    let mut plan = fixture(b.path(), true);
    plan.packages
        .iter_mut()
        .find(|p| p.identity == "test:common@1")
        .unwrap()
        .identity = "test:common@2".into();
    assert_ne!(symbols(&first), symbols(&lower(&plan)));
}

#[test]
fn canonical_type_arguments_preserve_aliases_and_nominal_distinctions() {
    let root = tempfile::tempdir().unwrap();
    let plan = fixture(root.path(), false);
    let main = &plan.files[0].path;
    std::fs::write(
        main,
        r#"
comp Alias: type = int32;
comp First: type = struct { value: int32 };
comp Second: type = struct { value: int32 };
comp Node: type = struct { next: *Node };
comp NodeAlias: type = Node;
comp Callback: type = fn(int32) int32;
comp CCallback: type = callconv "C" fn(int32) int32;
comp fn Marker(T:type) type { return struct {}; }
comp A: type = Marker(int32);
comp B: type = Marker(int64);
fn consume(T:type) int32 { return 0; }
fn main() int32 {
    return consume(int32)+consume(Alias)+consume(First)+consume(Second)
        +consume(A)+consume(B)+consume(*int32)+consume(int32[])
        +consume(const int32[])+consume(int32[2])+consume(int32[3])
        +consume(*Alias)+consume(Alias[])+consume(Alias[2])
        +consume(Node)+consume(NodeAlias)+consume(*Node)+consume(*NodeAlias)
        +consume(Callback)+consume(CCallback);
}
"#,
    )
    .unwrap();
    let ir = lower(&plan);
    let names: Vec<_> = ir
        .functions
        .iter()
        .filter(|f| {
            f.identifier.starts_with("fib.v1.instance.") && f.identifier.contains("7_consume")
        })
        .map(|f| &f.identifier)
        .collect();
    assert_eq!(names.len(), 14);
    assert_eq!(
        names.iter().collect::<std::collections::HashSet<_>>().len(),
        14
    );
    let mut source = std::fs::read_to_string(main).unwrap();
    source.push_str("comp AAA:type = First; comp fn AAAWrapper(T:type) type { return Marker(T); } comp C:type = AAAWrapper(int32);");
    source = source.replace("consume(Alias)", "consume(Alias)+consume(AAA)+consume(C)");
    std::fs::write(main, source).unwrap();
    assert_eq!(symbols(&ir), symbols(&lower(&plan)));
}

#[test]
fn unicode_modules_and_dependency_aliases_normalize_and_reject_collisions() {
    let root = tempfile::tempdir().unwrap();
    let mut plan = fixture(root.path(), false);
    let main = plan.files[0].path.clone();
    let old = plan.files[3].path.clone();
    let renamed = old.with_file_name("Cafe\u{301}.fib");
    std::fs::rename(old, &renamed).unwrap();
    plan.files[3].path = renamed;
    plan.packages[0]
        .dependencies
        .insert("Straße".into(), PackageId(3));
    std::fs::write(&main, "fn main() int32 { return STRASSE::CAFÉ::value(); }").unwrap();
    // Other packages have private dependency environments; update their paths too.
    for i in [1, 2] {
        std::fs::write(
            &plan.files[i].path,
            "pub comp value := shared::café::value;",
        )
        .unwrap();
    }
    let ir = lower(&plan);
    assert!(symbols(&ir).iter().any(|name| name.contains("5_café")));
    plan.packages[0]
        .dependencies
        .insert("STRASSE".into(), PackageId(1));
    assert!(run_pipeline(&plan, PipelineGoal::Ir).is_err());
    plan.packages[0].dependencies.remove("STRASSE");
    let collision = plan.packages[3].source_root.join("CAFÉ.fib");
    std::fs::write(&collision, "pub fn another() {}").unwrap();
    plan.files.push(SourceFile {
        id: FileId(4),
        name: "collision".to_string().into(),
        path: collision,
    });
    plan.packages[3].files.push(FileId(4));
    assert!(run_pipeline(&plan, PipelineGoal::Ir).is_err());
}

#[test]
fn generic_local_nominal_types_keep_the_enclosing_arguments() {
    let root = tempfile::tempdir().unwrap();
    let plan = fixture(root.path(), false);
    std::fs::write(&plan.files[0].path, "fn consume(T:type) int32 { return 0; } fn outer(T:type) int32 { comp Local:type = struct {}; return consume(Local); } fn main() int32 { return outer(int32)+outer(int64); }").unwrap();
    let ir = lower(&plan);
    // The checked symbolic body also creates an independent nominal instance.
    assert_eq!(
        ir.functions
            .iter()
            .filter(|f| f.identifier.starts_with("fib.v1.instance.")
                && f.identifier.contains("7_consume"))
            .count(),
        3
    );
}

#[test]
fn foreign_symbols_keep_their_spelling_and_collisions_are_errors() {
    let root = tempfile::tempdir().unwrap();
    let mut plan = fixture(root.path(), false);
    let main = plan
        .files
        .iter()
        .find(|f| f.id == plan.entry)
        .unwrap()
        .path
        .clone();
    std::fs::write(
        &main,
        "callconv \"C\" fn foreign(value:int32) int32; fn main() int32 { return 0; }",
    )
    .unwrap();
    assert!(
        lower(&plan)
            .functions
            .iter()
            .any(|f| f.identifier == "foreign" && matches!(f.body, FunctionBody::Declaration))
    );
    let other = plan.files[1].path.clone();
    for source in [
        "callconv \"C\" fn foreign(value:int32) int32;",
        "callconv \"C\" fn foreign(value:int64) int32;",
        "callconv \"C\" fn main() int32 { return 1; }",
    ] {
        std::fs::write(&other, source).unwrap();
        let error = run_pipeline(&plan, PipelineGoal::Ir).unwrap_err();
        assert!(format!("{error:#}").contains("duplicate"), "{error:#}");
    }
    plan.packages[1].identity = plan.packages[0].identity.clone();
    assert!(run_pipeline(&plan, PipelineGoal::Ir).is_err());
}

#[test]
fn package_versions_coexist_and_invalid_persistent_identities_fail() {
    let root = tempfile::tempdir().unwrap();
    let mut plan = fixture(root.path(), false);
    let older_root = root.path().join("older");
    std::fs::create_dir(&older_root).unwrap();
    let older_source = older_root.join("values.fib");
    std::fs::copy(&plan.files[3].path, &older_source).unwrap();
    plan.files.push(SourceFile {
        id: FileId(4),
        name: "unused".to_string().into(),
        path: older_source,
    });
    plan.packages.push(PackageSpec {
        id: PackageId(4),
        identity: "test:common@0".into(),
        source_root: older_root,
        dependencies: Default::default(),
        files: vec![FileId(4)],
    });
    plan.packages[0]
        .dependencies
        .insert("older".into(), PackageId(4));
    std::fs::write(
        &plan.files[0].path,
        "fn main() int32 { return older::values::value()+dep_left::api::value(); }",
    )
    .unwrap();
    let ir = lower(&plan);
    assert_eq!(
        ir.functions
            .iter()
            .filter(|f| f.identifier.ends_with("5_value"))
            .count(),
        2
    );
    for identity in ["", "bad\nidentity", "test:common@1"] {
        plan.packages[4].identity = identity.into();
        let error = run_pipeline(&plan, PipelineGoal::Ir).unwrap_err();
        assert!(
            format!("{error:#}").contains("persistent package identity"),
            "{error:#}"
        );
    }
}

#[cfg(feature = "llvm")]
#[test]
fn diamond_aliases_and_same_spelled_helpers_link_and_execute() {
    for alternate in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let plan = fixture(root.path(), alternate);
        run_pipeline(&plan, PipelineGoal::Binary).unwrap();
        assert_eq!(
            std::process::Command::new(&plan.output)
                .status()
                .unwrap()
                .code(),
            Some(42)
        );
    }
}

#[cfg(feature = "llvm")]
#[test]
fn foreign_declarations_link_to_the_declared_native_symbol() {
    let root = tempfile::tempdir().unwrap();
    let mut plan = fixture(root.path(), false);
    std::fs::write(
        &plan.files[0].path,
        "callconv \"C\" fn foreign(value:int32) int32; fn main() int32 { return foreign(41); }",
    )
    .unwrap();
    let native = root.path().join("foreign.c");
    std::fs::write(&native, "int foreign(int value) { return value + 1; }").unwrap();
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
