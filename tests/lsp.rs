//! Real framed-stdio checks for the native server, without LLVM.
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

struct Client {
    child: Child,
    input: ChildStdin,
    messages: Receiver<Value>,
}
impl Client {
    fn new(root: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_fib-lsp"))
            .env(
                "FIB_SYSROOT",
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("library"),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
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
                let mut body = vec![0; length.expect("stdout must contain only LSP frames")];
                output.read_exact(&mut body).unwrap();
                if sender.send(serde_json::from_slice(&body).unwrap()).is_err() {
                    return;
                }
            }
        });
        let mut client = Self {
            child,
            input,
            messages,
        };
        client.send(
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"rootUri":uri(root)}}),
        );
        let initialized = client.wait(|m| m["id"] == 1);
        assert_eq!(initialized["result"]["serverInfo"]["name"], "fib-lsp");
        assert_eq!(
            initialized["result"]["capabilities"]["completionProvider"]["triggerCharacters"],
            json!([":"])
        );
        assert_eq!(
            initialized["result"]["capabilities"]["positionEncoding"],
            "utf-16"
        );
        client.send(json!({"jsonrpc":"2.0","method":"initialized","params":{}}));
        client
    }
    fn send(&mut self, message: Value) {
        let body = message.to_string();
        write!(self.input, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
        self.input.flush().unwrap();
    }
    fn wait(&self, predicate: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let message = self
                .messages
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("LSP response timed out");
            if predicate(&message) {
                return message;
            }
        }
    }
    fn open(&mut self, path: &Path, source: &str) {
        self.send(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri(path),"version":1,"languageId":"fib","text":source}}}));
    }
    fn change(&mut self, path: &Path, source: &str, version: i64) {
        self.send(json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri(path),"version":version},"contentChanges":[{"text":source}]}}));
    }
    fn published(&self, path: &Path, version: i64) -> Value {
        self.wait(|m| {
            m["method"] == "textDocument/publishDiagnostics"
                && m["params"]["uri"] == uri(path)
                && m["params"]["version"] == version
        })
    }
    fn finish(mut self) {
        self.send(json!({"jsonrpc":"2.0","id":999,"method":"shutdown"}));
        assert!(self.wait(|m| m["id"] == 999)["result"].is_null());
        self.send(json!({"jsonrpc":"2.0","method":"exit"}));
        assert!(self.child.wait().unwrap().success());
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn uri(path: &Path) -> String {
    format!("file://{}", path.display()).replace(' ', "%20")
}

#[test]
fn lifecycle_multiple_errors_edits_tokens_and_close() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("not yet saved");
    std::fs::create_dir(&workspace).unwrap();
    let path = workspace.join("main.fib");
    let mut client = Client::new(root.path());
    client.open(&path, "fn f() { missing; another; }");
    let errors = client.published(&path, 1);
    assert_eq!(
        errors["params"]["diagnostics"].as_array().unwrap().len(),
        2,
        "{errors}"
    );
    assert_eq!(errors["params"]["diagnostics"][0]["code"], "name");
    client.change(&path, "fn f() { x: int32 = 1; }", 2);
    assert_eq!(
        client.published(&path, 2)["params"]["diagnostics"],
        json!([])
    );
    client.send(json!({"jsonrpc":"2.0","id":2,"method":"textDocument/semanticTokens/full","params":{"textDocument":{"uri":uri(&path)}}}));
    assert!(
        !client.wait(|m| m["id"] == 2)["result"]["data"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    client.send(json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri(&path)}}}));
    assert_eq!(
        client.wait(|m| m["method"] == "textDocument/publishDiagnostics")["params"]["diagnostics"],
        json!([])
    );
    client.send(json!({"jsonrpc":"2.0","id":3,"method":"textDocument/semanticTokens/full","params":{"textDocument":{"uri":uri(&path)}}}));
    assert_eq!(client.wait(|m| m["id"] == 3)["error"]["code"], -32602);
    client.finish();
}

#[test]
fn installed_std_resolves_in_unsaved_documents_and_keeps_private_helpers_private() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("syscalls.fib");
    let mut client = Client::new(root.path());
    client.open(
        &path,
        "fn main() int32 { return std::os::syscall0(39) as int32; }",
    );
    let valid = client.published(&path, 1);
    assert!(
        valid["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty(),
        "{valid}"
    );
    client.change(
        &path,
        "fn main() int32 { p := std::os_abi::pointer_from_word(0); return 0; }",
        2,
    );
    let errors = client.published(&path, 2);
    assert!(
        errors["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["message"].as_str().is_some_and(|m| m.contains("private"))),
        "{errors}"
    );
}

#[test]
fn editing_std_uses_its_own_manifest_and_unsaved_sibling_sources() {
    let directory = tempfile::tempdir().unwrap();
    let sysroot = directory.path().join("local-library");
    let sources = sysroot.join("std");
    std::fs::create_dir_all(&sources).unwrap();
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("library");
    std::fs::copy(
        repository.join("sysroot.toml"),
        sysroot.join("sysroot.toml"),
    )
    .unwrap();
    for name in [
        "os", "memory", "result", "os_abi", "prelude", "io", "map", "process",
    ] {
        std::fs::copy(
            repository.join(format!("std/{name}.fib")),
            sources.join(format!("{name}.fib")),
        )
        .unwrap();
    }
    // A source package is independent of an enclosing application's build file.
    std::fs::write(directory.path().join("build.fib"), "invalid build file").unwrap();
    let os = sources.join("os.fib");
    let result = sources.join("result.fib");
    let mut client = Client::new(directory.path());
    client.open(&os, &std::fs::read_to_string(&os).unwrap());
    let valid = client.published(&os, 1);
    assert_eq!(valid["params"]["diagnostics"], json!([]), "{valid}");
    client.open(
        &result,
        "pub comp fn Result(T: type, E: type) type { return bool; }",
    );
    let invalid = client.published(&os, 1);
    assert!(
        invalid["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "type"),
        "{invalid}"
    );
    client.change(&result, &std::fs::read_to_string(&result).unwrap(), 2);
    let restored = client.published(&os, 1);
    assert_eq!(restored["params"]["diagnostics"], json!([]), "{restored}");
    client.finish();
}

#[test]
fn project_analysis_uses_unsaved_dependency_and_build_file() {
    let root = tempfile::tempdir().unwrap();
    let main = root.path().join("main.fib");
    let helper = root.path().join("helper.fib");
    let build = root.path().join("build.fib");
    std::fs::write(&main, "fn main() int32 { return helper::answer(); }").unwrap();
    std::fs::write(&helper, "pub fn answer() int32 { return 42; }").unwrap();
    let valid_build = "fn build() int32 { build_api::add_source(\"main.fib\"[..]); build_api::add_source(\"helper.fib\"[..]); build_api::set_entry(\"main\"[..]); build_api::set_output(\"app\"[..]); return 0; }";
    std::fs::write(&build, valid_build).unwrap();
    let mut client = Client::new(root.path());
    client.open(&main, "fn main() int32 { return helper::answer(); }");
    assert_eq!(
        client.published(&main, 1)["params"]["diagnostics"],
        json!([])
    );
    client.open(&helper, "pub fn answer() int32 { return false; }");
    assert!(
        !client.published(&helper, 1)["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    client.open(&build, "fn build() int32 { unknown; another; return 0; }");
    assert_eq!(
        client.published(&build, 1)["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    client.change(&build, valid_build, 2);
    assert_eq!(
        client.published(&build, 2)["params"]["diagnostics"],
        json!([])
    );
    client.finish();
}

#[test]
fn rapid_edits_publish_latest_snapshot_and_watched_files_reload() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("main.fib");
    let mut client = Client::new(root.path());
    client.open(&path, "fn f() { bad; }");
    for version in 2..20 {
        client.change(&path, "fn f() { worse; }", version);
    }
    client.change(&path, "fn f() {}", 20);
    assert_eq!(
        client.published(&path, 20)["params"]["diagnostics"],
        json!([])
    );
    client.change(&path, "fn f() { stale; }", 19);
    client.send(
        json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles","params":{"changes":[]}}),
    );
    assert_eq!(
        client.published(&path, 20)["params"]["diagnostics"],
        json!([])
    );
    client.finish();
}

#[test]
fn watched_disk_dependencies_and_removed_members_clear_diagnostics() {
    let root = tempfile::tempdir().unwrap();
    let main = root.path().join("main.fib");
    let helper = root.path().join("helper.fib");
    let build = root.path().join("build.fib");
    let main_source = "fn main() int32 { return helper::answer(); }";
    std::fs::write(&main, main_source).unwrap();
    std::fs::write(&helper, "pub fn answer() int32 { return false; }").unwrap();
    std::fs::write(&build, "fn build() int32 { build_api::add_source(\"main.fib\"[..]); build_api::add_source(\"helper.fib\"[..]); build_api::set_entry(\"main\"[..]); build_api::set_output(\"app\"[..]); return 0; }").unwrap();
    let mut client = Client::new(root.path());
    client.open(&main, main_source);
    let helper_uri = uri(&helper);
    let first = client.wait(|m| m["params"]["uri"] == helper_uri);
    assert!(
        !first["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    std::fs::write(&helper, "pub fn answer() int32 { return 42; }").unwrap();
    client.send(json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles","params":{"changes":[{"uri":helper_uri,"type":2}]}}));
    assert_eq!(
        client.wait(|m| m["params"]["uri"] == helper_uri)["params"]["diagnostics"],
        json!([])
    );
    client.open(&build, "fn build() int32 { build_api::add_source(\"main.fib\"[..]); build_api::set_entry(\"main\"[..]); build_api::set_output(\"app\"[..]); return 0; }");
    assert_eq!(
        client.wait(|m| m["params"]["uri"] == helper_uri)["params"]["diagnostics"],
        json!([])
    );
    assert!(!root.path().join("app").exists());
    assert!(!root.path().join("app.o").exists());
    client.finish();
}

#[test]
fn diagnostics_convert_unicode_columns_to_utf16() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("unicode.fib");
    let source = "fn f() { s := \"😀\"; missing; }";
    let mut client = Client::new(root.path());
    client.open(&path, source);
    let diagnostics = client.published(&path, 1);
    let error = &diagnostics["params"]["diagnostics"][0];
    let start = source[..source.find("missing").unwrap()]
        .encode_utf16()
        .count();
    assert_eq!(error["range"]["start"]["character"], start);
    assert_eq!(error["range"]["end"]["character"], start + "missing".len());
    client.finish();
}

#[test]
fn completes_std_paths_in_unfinished_unsaved_expressions() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("main.fib");
    let mut client = Client::new(root.path());
    for (version, suffix, expected, kind) in [
        (1, "std::", "io", 9),
        (2, "std::io::", "print", 3),
        (3, "std::io::pri", "printf", 3),
        (4, "std::os::", "syscall0", 3),
    ] {
        let source = format!("fn f() {{ s := \"😀\"; {suffix}");
        if version == 1 {
            client.open(&path, &source);
        } else {
            client.change(&path, &source, version);
        }
        // Request immediately, before background analysis has finished.
        client.send(json!({"jsonrpc":"2.0","id":version + 20,"method":"textDocument/completion","params":{"textDocument":{"uri":uri(&path)},"position":{"line":0,"character":source.encode_utf16().count()}}}));
        let response = client.wait(|m| m["id"] == version + 20);
        let items = response["result"]["items"].as_array().unwrap();
        assert!(
            items
                .iter()
                .any(|i| i["label"] == expected && i["kind"] == kind),
            "{response}"
        );
        if suffix.ends_with("pri") {
            assert!(
                items
                    .iter()
                    .all(|i| i["label"].as_str().unwrap().starts_with("pri"))
            );
        }
    }
    client.change(&path, "fn f() { std::os_abi::", 5);
    client.send(json!({"jsonrpc":"2.0","id":30,"method":"textDocument/completion","params":{"textDocument":{"uri":uri(&path)},"position":{"line":0,"character":21}}}));
    assert_eq!(client.wait(|m| m["id"] == 30)["result"]["items"], json!([]));
    client.finish();
}
