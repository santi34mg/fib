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

fn input(source: &str) -> (tempfile::TempDir, BuildPlan) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("main.fib");
    std::fs::write(&path, source).unwrap();
    let mut plan = BuildPlan::single_file(&path).unwrap();
    plan.standard_library = StandardLibrary::at(sysroot());
    (directory, plan)
}

fn analyze(plan: &BuildPlan) -> anyhow::Result<AnalyzedProject> {
    let prepared = plan.prepare()?;
    Analyzer::new(prepared.clone(), parse_files(&prepared)?)?.analyze()
}

#[test]
fn memory_byte_operations_accept_writable_destinations_and_readonly_sources() {
    let (_directory, plan) = input(include_str!("../samples/memory.fib"));
    fibc::ir::verify_ir(&fibc::ir::lowering::lower(&analyze(&plan).unwrap()).unwrap()).unwrap();
    for source in [
        "fn main() { std::memory::memset(\"hi\"[..], 0); }",
        "fn main() { std::memory::memcpy(\"hi\"[..], \"ok\"[..]); }",
        "fn main() { a: uint16[1] = [0]; std::memory::memset(a[..], 0); }",
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
            "fn main() int32 { a: uint8[1] = [0]; std::memory::memcpy(a[..], \"hi\"[..]); return 1; }",
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
            switch (io::write_all(1, "abcde"[..])) {
                when .Err(e) { return 1; }
                when .Ok(n) { if n != 5 || std::os::test_calls() != 4 { return 2; }; }
            };
            std::os::test_reset(1);
            switch (io::write_all(1, "x"[..])) {
                when .Ok(n) { return 3; }
                when .Err(e) {
                    switch (e) {
                        when .Io(errno) { if errno != 5 { return 4; }; }
                        when .InvalidFormat { return 5; }
                        when .ArgumentCount { return 6; }
                    };
                }
            };
            std::os::test_reset(2);
            switch (io::write_all(1, "x"[..])) {
                when .Ok(n) { return 7; }
                when .Err(e) {
                    switch (e) {
                        when .Io(errno) { if errno != 28 { return 8; }; }
                        when .InvalidFormat { return 9; }
                        when .ArgumentCount { return 10; }
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
    for name in ["memory", "os", "os_abi", "prelude", "result", "io"] {
        std::fs::copy(
            sysroot().join(format!("std/{name}.fib")),
            installed.join(format!("std/{name}.fib")),
        )
        .unwrap();
    }
    // Script the write boundary to force otherwise timing-dependent outcomes.
    let path = installed.join("std/os.fib");
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
        pub fn write(fd: int32, buffer: const uint8[]) CountResult {
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
    let path = directory.path().join("nested/helper.fib");
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
    for name in ["os", "memory", "result", "os_abi", "prelude"] {
        let path = sysroot().join(format!("std/{name}.fib"));
        // The owning source tree takes precedence over the server's installation.
        let plan = tooling::standalone_plan(&path)
            .with_standard_library(StandardLibrary::at("/missing/installed/sysroot"));
        let prepared = plan.prepare().unwrap();
        assert_eq!(prepared.entry, plan.entry);
        assert_eq!(prepared.files.len(), 6);
        assert_eq!(prepared.packages.len(), 1);
        assert_eq!(prepared.files.iter().filter(|f| f.path == path).count(), 1);
        assert!(prepared.packages[0].files.contains(&plan.entry));
        assert_eq!(prepared.prepare().unwrap().files, prepared.files);
        analyze(&prepared).unwrap();
        let report = tooling::analyze_plan(&plan, &DiskSources, false);
        assert!(
            report.documents[&path].diagnostics.is_empty(),
            "{name}: {:?}",
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
    for name in ["memory", "os", "os_abi", "prelude", "result", "io"] {
        std::fs::copy(
            sysroot().join(format!("std/{name}.fib")),
            installed.join(format!("std/{name}.fib")),
        )
        .unwrap();
    }
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

#[cfg(feature = "llvm")]
#[test]
fn allocator_and_sbrk_execute_against_installed_sources() {
    for source in [
        include_str!("../samples/allocator.fib"),
        include_str!("programs/sbrk.fib"),
        r#"
        comp m := std::memory;
        fn main() int32 {
            // Force an unaligned initial break before the allocator starts.
            switch (std::os::sbrk(1)) { when .Ok(p) {} when .Err(e) { return 1; } };
            big := m::alloc(uint8, 256);
            guard := m::alloc(uint128, 2);
            if big.len != 256 || guard.len != 2 { return 2; };
            guard[0] = 42;
            original := m::slice_data(big);
            m::free(uint8, original);
            left := m::alloc(uint8, 64);
            right := m::alloc(uint8, 64);
            if left.len != 64 || right.len != 64 || m::slice_data(left) != original { return 3; };
            if m::slice_data(left) == m::slice_data(right) { return 4; };
            left[63] = 41;
            right[0] = 42;
            if left[63] != 41 || right[0] != 42 || guard[0] != 42 { return 5; };
            m::free(uint8, m::slice_data(right));
            m::free(uint8, m::slice_data(left));
            merged := m::alloc(uint8, 256);
            if merged.len != 256 || m::slice_data(merged) != original { return 6; };
            m::free(uint8, m::slice_data(merged));
            m::free(uint128, m::slice_data(guard));
            // Adjacent independently requested regions also coalesce.
            larger := m::alloc(uint8, 300);
            if larger.len != 300 || m::slice_data(larger) != original { return 7; };
            larger[299] = 42;
            // An unrelated pointer must not corrupt the list.
            local := 1;
            m::free(int32, local&);
            m::free(uint8, m::slice_data(larger));
            again := m::alloc(uint8, 300);
            if again.len != 300 || m::slice_data(again) != original { return 8; };
            return 42;
        }
        "#,
        r#"
        fn main() int32 {
            // Reject multiplication beyond the signed sbrk limit before asking Linux.
            huge := std::memory::alloc(uint64[1073741824], 4294967295);
            if huge.len != 0 { return 1; };
            live := std::memory::alloc(uint8, 16);
            if live.len != 16 { return 4; };
            live[0] = 42;
            // setrlimit(RLIMIT_DATA, {0, 0}) makes brk growth fail deterministically.
            limits: uint64[2] = [0, 0];
            if std::os::syscall2(160, 2, limits&) != 0 { return 2; };
            failed := std::memory::alloc(uint8, 32);
            if failed.len != 0 { return 3; };
            if live[0] != 42 { return 5; };
            original := std::memory::slice_data(live);
            std::memory::free(uint8, original);
            reused := std::memory::alloc(uint8, 16);
            if reused.len != 16 || std::memory::slice_data(reused) != original { return 6; };
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

#[cfg(feature = "llvm")]
#[test]
fn syscall_wrappers_execute_without_a_native_shim() {
    use fibc::driver::{PipelineGoal, run_pipeline};
    let (_directory, plan) = input(include_str!("programs/standard_library.fib"));
    run_pipeline(&plan, PipelineGoal::Binary).unwrap();
    let output = std::process::Command::new(&plan.output).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(output.stdout, b"std syscalls\n");
}

#[cfg(feature = "llvm")]
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

#[cfg(feature = "llvm")]
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
