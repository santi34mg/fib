//! Dependency-free, opt-in wall-clock benchmarks. See docs/performance.md.
use fibc::{
    driver::parse_files_with_sources,
    frontend::{
        analyzer::{Analyzer, validate_analysis},
        parser::Parser,
        source::OverlaySources,
        tokenizer::Tokenizer,
        tooling::{self, Position},
    },
    standard_library::StandardLibrary,
};
use serde_json::{Value, json};
use std::{
    hint::black_box,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

fn selected(name: &str) -> bool {
    !std::env::var("FIB_BENCH_FILTER")
        .is_ok_and(|filter| !filter.split(',').any(|part| name.contains(part)))
}

fn measure<S, R>(name: &str, mut setup: impl FnMut() -> S, mut run: impl FnMut(S) -> R) {
    if !selected(name) {
        return;
    }
    let count = std::env::var("FIB_BENCH_SAMPLES")
        .map(|s| s.parse::<usize>().expect("positive sample count"))
        .unwrap_or(30);
    assert!(count > 0);
    for _ in 0..3 {
        black_box(run(setup()));
    }
    let mut samples = Vec::with_capacity(count);
    for _ in 0..count {
        let state = setup();
        let start = Instant::now();
        let result = black_box(run(state));
        samples.push(start.elapsed().as_secs_f64() * 1000.0);
        // Destruction of returned results is outside the measurement.
        drop(result);
    }
    samples.sort_by(f64::total_cmp);
    let percentile = |p: usize| samples[(count * p).div_ceil(100).saturating_sub(1)];
    println!(
        "{name},{count},{:.6},{:.6},{:.6},{:.6}",
        samples[0],
        percentile(50),
        percentile(95),
        samples.iter().sum::<f64>() / count as f64
    );
}

fn source(functions: usize) -> String {
    let mut text = String::new();
    for i in 0..functions {
        text.push_str(&format!("fn helper_{i}(x: int32) int32 {{ y := x + 1; if (y > 10) {{ return y - 2; }} else {{ return y * 2; }}; }}\n"));
    }
    text.push_str("fn main() int32 { return helper_0(2); }\n");
    text
}

fn standard_library(enabled: bool) -> StandardLibrary {
    let mut library = StandardLibrary::default();
    library.enabled = enabled;
    library.sysroot = Some(Path::new(env!("CARGO_MANIFEST_DIR")).join("library"));
    library
}

fn compiler() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("main.fib");
    for size in [10, 100, 500] {
        let source = source(size);
        eprintln!("fixture functions={size} bytes={}", source.len());
        let label = format!("compiler/{size}");
        measure(
            &format!("{label}/tokenize"),
            || (),
            |_| Tokenizer::new(black_box(&source)).collect::<Vec<_>>(),
        );
        measure(
            &format!("{label}/parse_including_tokenize"),
            || (),
            |_| {
                Parser::with_source(Tokenizer::new(&source), &path, &source)
                    .parse()
                    .unwrap()
            },
        );
        let sources = OverlaySources([(path.clone(), source)].into_iter().collect());
        for std in [false, true] {
            let mode = if std { "std" } else { "no_std" };
            let label = format!("{label}/{mode}");
            let plan = tooling::standalone_plan(&path).with_standard_library(standard_library(std));
            measure(
                &format!("{label}/prepare"),
                || (),
                |_| plan.prepare().unwrap(),
            );
            let prepared = plan.prepare().unwrap();
            measure(
                &format!("{label}/parse_plan"),
                || (),
                |_| parse_files_with_sources(&prepared, &sources).unwrap(),
            );
            let units = parse_files_with_sources(&prepared, &sources).unwrap();
            measure(
                &format!("{label}/collect_declarations"),
                || Analyzer::new(prepared.clone(), units.clone()).unwrap(),
                |mut analyzer| {
                    analyzer.collect_declarations().unwrap();
                    analyzer
                },
            );
            measure(
                &format!("{label}/resolve_names"),
                || {
                    let mut analyzer = Analyzer::new(prepared.clone(), units.clone()).unwrap();
                    analyzer.collect_declarations().unwrap();
                    analyzer
                },
                |mut analyzer| {
                    analyzer.resolve_names().unwrap();
                    analyzer
                },
            );
            measure(
                &format!("{label}/resolve_types"),
                || {
                    let mut analyzer = Analyzer::new(prepared.clone(), units.clone()).unwrap();
                    analyzer.collect_declarations().unwrap();
                    analyzer.resolve_names().unwrap();
                    analyzer
                },
                |mut analyzer| {
                    let types = analyzer.resolve_types().unwrap();
                    (analyzer, types)
                },
            );
            measure(
                &format!("{label}/analyze_strict"),
                || (prepared.clone(), units.clone()),
                |(p, u)| Analyzer::new(p, u).unwrap().analyze().unwrap(),
            );
            measure(
                &format!("{label}/analyze_collecting"),
                || (prepared.clone(), units.clone()),
                |(p, u)| {
                    let report = Analyzer::new(p, u).unwrap().analyze_collecting();
                    assert!(report.errors.is_empty(), "{:?}", report.errors);
                    report
                },
            );
            let analyzed = Analyzer::new(prepared.clone(), units)
                .unwrap()
                .analyze()
                .unwrap();
            measure(
                &format!("{label}/clone_semantic_types"),
                || (),
                |_| analyzed.types.clone(),
            );
            measure(
                &format!("{label}/clone_analyzed_units"),
                || (),
                |_| analyzed.units.clone(),
            );
            measure(
                &format!("{label}/validate_analysis"),
                || (),
                |_| validate_analysis(&analyzed).unwrap(),
            );
            measure(
                &format!("{label}/lower_including_validation"),
                || (),
                |_| fibc::ir::lowering::lower(&analyzed).unwrap(),
            );
            measure(
                &format!("{label}/frontend_to_ir"),
                || (),
                |_| {
                    let plan = plan.prepare().unwrap();
                    let units = parse_files_with_sources(&plan, &sources).unwrap();
                    let project = Analyzer::new(plan, units).unwrap().analyze().unwrap();
                    fibc::ir::lowering::lower(&project).unwrap()
                },
            );
            let ir = fibc::ir::lowering::lower(&analyzed).unwrap();
            measure(&format!("{label}/clone_ir"), || (), |_| ir.clone());
            measure(
                &format!("{label}/verify_ir"),
                || (),
                |_| fibc::ir::verify_ir(&ir).unwrap(),
            );
            #[cfg(feature = "llvm")]
            {
                let target = fibc::backend::lowering::llvm_lower::host_target().unwrap();
                measure(
                    &format!("{label}/emit_llvm"),
                    || (),
                    |_| fibc::backend::emit_llvm(&ir, &target).unwrap(),
                );
                let artifact = fibc::backend::emit_llvm(&ir, &target).unwrap();
                let object = directory.path().join("bench.o");
                measure(
                    &format!("{label}/write_object"),
                    || (),
                    |_| artifact.write_object(&object).unwrap(),
                );
                let binary = std::env::current_exe()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .join("fibc");
                std::fs::write(&path, &sources.0[&path]).unwrap();
                measure(
                    &format!("{label}/cli_build_and_link"),
                    || (),
                    |_| {
                        let mut command = Command::new(&binary);
                        command.arg("build").arg("--single-file").arg(&path);
                        if std {
                            command
                                .arg("--sysroot")
                                .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("library"));
                        } else {
                            command.arg("--no-std");
                        }
                        let output = command.output().unwrap();
                        assert!(
                            output.status.success(),
                            "{}",
                            String::from_utf8_lossy(&output.stderr)
                        );
                        output
                    },
                );
            }
            measure(
                &format!("{label}/editor_analysis"),
                || (),
                |_| {
                    let result = tooling::analyze_plan(&plan, &sources, false);
                    assert!(
                        result
                            .documents
                            .values()
                            .all(|d| d.diagnostics.iter().all(|d| d.warning))
                    );
                    result
                },
            );
            let result = tooling::analyze_plan(&plan, &sources, false);
            let document = &result.documents[&path];
            measure(
                &format!("{label}/semantic_token_encoding"),
                || (),
                |_| tooling::encode_tokens(&document.source, &document.tokens, &Default::default()),
            );
        }
    }
    for name in ["generic_map", "process_entry"] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("samples/{name}.fib"));
        let sources = OverlaySources::default();
        let plan = tooling::standalone_plan(&path).with_standard_library(standard_library(true));
        let prepared = plan.prepare().unwrap();
        let units = parse_files_with_sources(&prepared, &sources).unwrap();
        measure(
            &format!("compiler/sample_{name}/analyze_strict"),
            || (prepared.clone(), units.clone()),
            |(plan, units)| Analyzer::new(plan, units).unwrap().analyze().unwrap(),
        );
        measure(
            &format!("compiler/sample_{name}/editor_analysis"),
            || (),
            |_| {
                let result = tooling::analyze_plan(&plan, &sources, false);
                assert!(
                    result
                        .documents
                        .values()
                        .all(|d| d.diagnostics.iter().all(|d| d.warning))
                );
                result
            },
        );
    }
    // Isolate cached completion lookup from full analysis and std loading.
    let text = "fn f() { std::io::pri";
    let plan = tooling::standalone_plan(&path).with_standard_library(standard_library(true));
    let sources = OverlaySources([(path.clone(), text.to_owned())].into_iter().collect());
    let result = tooling::analyze_plan(&plan, &sources, false);
    let document = &result.documents[&path];
    measure(
        "lsp_service/cached_completion_1000_requests",
        || (),
        |_| {
            for _ in 0..1000 {
                let items = tooling::complete(
                    document,
                    Position {
                        line: 0,
                        character: text.len() as u32,
                    },
                );
                assert!(items.iter().any(|i| i.label == "print"));
                black_box(items);
            }
        },
    );
    for size in [10, 100, 500] {
        let text = format!("{}fn completion() {{ std::io::pri", source(size));
        let sources = OverlaySources([(path.clone(), text)].into_iter().collect());
        let result = tooling::analyze_plan(&plan, &sources, false);
        let document = &result.documents[&path];
        measure(
            &format!("lsp_service/{size}_functions/cached_completion_100_requests"),
            || (),
            |_| {
                for _ in 0..100 {
                    let items = tooling::complete(
                        document,
                        Position {
                            line: size as u32 + 1,
                            character: "fn completion() { std::io::pri".len() as u32,
                        },
                    );
                    assert!(items.iter().any(|i| i.label == "print"));
                    black_box(items);
                }
            },
        );
    }
}

struct Client {
    child: Child,
    input: ChildStdin,
    messages: Receiver<Value>,
    next_id: u64,
}
impl Client {
    fn new(binary: &Path, root: &Path) -> Self {
        let mut child = Command::new(binary)
            .arg("--sysroot")
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("library"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("build release fib-lsp first; see docs/performance.md");
        let input = child.stdin.take().unwrap();
        let mut output = BufReader::new(child.stdout.take().unwrap());
        let (sender, messages) = mpsc::channel();
        thread::spawn(move || {
            loop {
                let mut length = None;
                loop {
                    let mut line = String::new();
                    if output.read_line(&mut line).unwrap() == 0 {
                        return;
                    }
                    if line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line.strip_prefix("Content-Length:") {
                        length = Some(value.trim().parse::<usize>().unwrap());
                    }
                }
                let mut bytes = vec![0; length.unwrap()];
                output.read_exact(&mut bytes).unwrap();
                if sender
                    .send(serde_json::from_slice(&bytes).unwrap())
                    .is_err()
                {
                    return;
                }
            }
        });
        let mut client = Self {
            child,
            input,
            messages,
            next_id: 1,
        };
        client.request("initialize", json!({"rootUri":uri(root)}));
        client.send(json!({"jsonrpc":"2.0","method":"initialized","params":{}}));
        client
    }
    fn send(&mut self, message: Value) {
        let body = message.to_string();
        write!(self.input, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
        self.input.flush().unwrap();
    }
    fn wait(&self, predicate: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let value = self
                .messages
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("LSP benchmark timed out");
            assert!(value.get("error").is_none(), "{value}");
            if predicate(&value) {
                return value;
            }
        }
    }
    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}));
        self.wait(|v| v["id"] == id)
    }
    fn open(&mut self, path: &Path, text: &str) {
        self.send(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri(path),"version":1,"languageId":"fib","text":text}}}));
        let diagnostics = self.diagnostics(path, 1);
        assert_eq!(diagnostics["params"]["diagnostics"], json!([]));
    }
    fn edit(&mut self, path: &Path, text: &str, version: i64) {
        self.send(json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri(path),"version":version},"contentChanges":[{"text":text}]}}));
    }
    fn diagnostics(&self, path: &Path, version: i64) -> Value {
        self.wait(|v| {
            v["method"] == "textDocument/publishDiagnostics"
                && v["params"]["uri"] == uri(path)
                && v["params"]["version"] == version
        })
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn uri(path: &Path) -> String {
    format!("file://{}", path.display())
        .replace('%', "%25")
        .replace(' ', "%20")
}

fn lsp() {
    let binary = std::env::var_os("FIB_BENCH_LSP")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::current_exe()
                .unwrap()
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("fib-lsp")
        });
    let root = tempfile::tempdir().unwrap();
    measure(
        "lsp_stdio/startup_initialize",
        || (),
        |_| Client::new(&binary, root.path()),
    );
    for count in [1, 5, 20] {
        let label = format!("lsp_stdio/{count}_standalone_documents");
        if ![
            "edit_to_diagnostics",
            "cached_completion",
            "edit_to_completion",
            "burst_10_edits_to_completion",
            "cached_semantic_tokens",
        ]
        .iter()
        .any(|name| selected(&format!("{label}/{name}")))
        {
            continue;
        }
        let mut client = Client::new(&binary, root.path());
        let paths: Vec<_> = (0..count)
            .map(|i| root.path().join(format!("file_{i}.fib")))
            .collect();
        let valid = source(10);
        for path in &paths {
            client.open(path, &valid);
        }
        let path = &paths[0];
        let mut version = 1;
        measure(
            &format!("{label}/edit_to_diagnostics"),
            || (),
            |_| {
                version += 1;
                client.edit(path, &format!("{valid}// version {version}\n"), version);
                let diagnostics = client.diagnostics(path, version);
                assert_eq!(diagnostics["params"]["diagnostics"], json!([]));
            },
        );
        let text = "fn f() { std::io::pri";
        version += 1;
        client.edit(path, text, version);
        client.diagnostics(path, version);
        let params =
            json!({"textDocument":{"uri":uri(path)},"position":{"line":0,"character":text.len()}});
        measure(
            &format!("{label}/cached_completion"),
            || (),
            |_| {
                let response = client.request("textDocument/completion", params.clone());
                assert!(
                    response["result"]["items"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|i| i["label"] == "print")
                );
                response
            },
        );
        measure(
            &format!("{label}/edit_to_completion"),
            || (),
            |_| {
                version += 1;
                client.edit(path, text, version);
                let response = client.request("textDocument/completion", params.clone());
                assert!(
                    response["result"]["items"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|i| i["label"] == "print")
                );
                response
            },
        );
        measure(
            &format!("{label}/burst_10_edits_to_completion"),
            || (),
            |_| {
                for _ in 0..10 {
                    version += 1;
                    client.edit(path, text, version);
                }
                let response = client.request("textDocument/completion", params.clone());
                assert!(
                    response["result"]["items"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|i| i["label"] == "print")
                );
                response
            },
        );
        measure(
            &format!("{label}/cached_semantic_tokens"),
            || (),
            |_| {
                client.request(
                    "textDocument/semanticTokens/full",
                    json!({"textDocument":{"uri":uri(path)}}),
                )
            },
        );
    }
    // Compare a shared explicit project with independent standalone plans.
    if !selected("lsp_stdio/5_project_documents/edit_to_diagnostics") {
        return;
    }
    let project = tempfile::tempdir().unwrap();
    let mut build = "fn build() int32 { ".to_owned();
    let mut paths = Vec::new();
    for i in 0..5 {
        let name = if i == 0 {
            "main".to_owned()
        } else {
            format!("helper_{i}")
        };
        let path = project.path().join(format!("{name}.fib"));
        std::fs::write(
            &path,
            if i == 0 {
                "fn main() int32 { return helper_1::answer(); }"
            } else {
                "pub fn answer() int32 { return 42; }"
            },
        )
        .unwrap();
        build.push_str(&format!("build_api::add_source(\"{name}.fib\"[..]); "));
        paths.push(path);
    }
    build.push_str(
        "build_api::set_entry(\"main\"[..]); build_api::set_output(\"app\"[..]); return 0; }",
    );
    std::fs::write(project.path().join("build.fib"), build).unwrap();
    let mut client = Client::new(&binary, project.path());
    for path in &paths {
        client.open(path, &std::fs::read_to_string(path).unwrap());
    }
    let source = std::fs::read_to_string(&paths[0]).unwrap();
    let mut version = 1;
    measure(
        "lsp_stdio/5_project_documents/edit_to_diagnostics",
        || (),
        |_| {
            version += 1;
            client.edit(
                &paths[0],
                &format!("{source}// version {version}\n"),
                version,
            );
            let diagnostics = client.diagnostics(&paths[0], version);
            assert_eq!(diagnostics["params"]["diagnostics"], json!([]));
        },
    );
}

fn main() {
    if !std::env::args().any(|arg| arg == "--bench") {
        println!("Performance measurements are opt-in: cargo bench --bench performance");
        return;
    }
    println!("benchmark,samples,min_ms,p50_ms,p95_ms,mean_ms");
    let filter = std::env::var("FIB_BENCH_FILTER").unwrap_or_default();
    if filter.is_empty() || filter.split(',').any(|part| !part.starts_with("lsp_stdio")) {
        compiler();
    }
    if filter.is_empty() || filter.split(',').any(|part| part.starts_with("lsp_stdio")) {
        lsp();
    }
}
