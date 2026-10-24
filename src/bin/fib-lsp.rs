//! Native stdio LSP. Compiler services own all language rules.
use fibc::{
    frontend::{
        source::OverlaySources,
        tooling::{self, ProjectAnalysis, SourceIndex},
    },
    plan::PlanBuilder,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashSet},
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
};

#[derive(Clone)]
struct Document {
    uri: String,
    path: PathBuf,
    text: String,
    version: i64,
}
#[derive(Clone)]
struct Snapshot {
    generation: u64,
    documents: BTreeMap<String, Document>,
    roots: Vec<PathBuf>,
}
enum Event {
    Message(Value),
    Invalid(String),
    Eof,
    Analysis(u64, ProjectAnalysis),
    Failure(u64, String),
}

fn send(message: Value) -> io::Result<()> {
    let body = serde_json::to_vec(&message)?;
    let mut out = io::stdout().lock();
    write!(out, "Content-Length: {}\r\n\r\n", body.len())?;
    out.write_all(&body)?;
    out.flush()
}
fn response(id: Value, result: Value) -> io::Result<()> {
    send(json!({"jsonrpc":"2.0", "id":id, "result":result}))
}
fn error(id: Value, code: i32, message: &str) -> io::Result<()> {
    send(json!({"jsonrpc":"2.0", "id":id, "error":{"code":code,"message":message}}))
}
fn diagnostics(uri: &str, version: Option<i64>, values: Vec<Value>) -> io::Result<()> {
    let mut params = json!({"uri":uri,"diagnostics":values});
    if let Some(version) = version {
        params["version"] = version.into();
    }
    send(json!({"jsonrpc":"2.0","method":"textDocument/publishDiagnostics","params":params}))
}

fn read_message(input: &mut impl BufRead) -> io::Result<Option<Value>> {
    let mut length = None;
    let mut header_bytes = 0;
    loop {
        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        header_bytes += line.len();
        if header_bytes > 8192 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "oversized LSP header",
            ));
        }
        if line == "\r\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("Content-Length")
        {
            length = Some(value.trim().parse::<usize>().map_err(io::Error::other)?);
        }
    }
    let length = length
        .filter(|n| *n <= 16 * 1024 * 1024)
        .ok_or_else(|| io::Error::other("missing or oversized Content-Length"))?;
    let mut bytes = vec![0; length];
    input.read_exact(&mut bytes)?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(io::Error::other)
}

fn uri_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let path = if let Some(path) = rest.strip_prefix("localhost/") {
        format!("/{path}")
    } else if rest.starts_with('/') {
        rest.to_owned()
    } else {
        return None;
    };
    if path.contains(['?', '#']) {
        return None;
    }
    let bytes = path.as_bytes();
    let mut decoded = Vec::new();
    let mut offset = 0;
    while offset < bytes.len() {
        if bytes[offset] == b'%' {
            let hex = std::str::from_utf8(bytes.get(offset + 1..offset + 3)?).ok()?;
            decoded.push(u8::from_str_radix(hex, 16).ok()?);
            offset += 3;
        } else {
            decoded.push(bytes[offset]);
            offset += 1;
        }
    }
    let decoded = String::from_utf8(decoded).ok()?;
    if decoded.contains('\0') {
        return None;
    }
    #[cfg(windows)]
    let decoded = decoded.strip_prefix('/').unwrap_or(&decoded).to_owned();
    let path = PathBuf::from(decoded);
    Some(path.canonicalize().unwrap_or(path))
}
fn path_uri(path: &Path) -> String {
    let mut uri = String::from("file://");
    #[cfg(windows)]
    uri.push('/');
    let text = path.to_string_lossy();
    #[cfg(windows)]
    let text = text.replace('\\', "/");
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~:".contains(&byte) {
            uri.push(byte as char);
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    uri
}

fn build_file(path: &Path, roots: &[PathBuf], sources: &OverlaySources) -> Option<PathBuf> {
    let boundary = roots
        .iter()
        .filter(|r| path.starts_with(r))
        .max_by_key(|r| r.components().count());
    for directory in path.parent()?.ancestors() {
        let candidate = directory.join("build.fib");
        if candidate.is_file() || sources.0.contains_key(&candidate) {
            return Some(candidate);
        }
        if boundary.is_some_and(|r| directory == r) {
            break;
        }
    }
    None
}
fn analyze(snapshot: Snapshot) -> ProjectAnalysis {
    let sources = OverlaySources(
        snapshot
            .documents
            .values()
            .map(|d| (d.path.clone(), d.text.clone()))
            .collect(),
    );
    let mut output = ProjectAnalysis::default();
    let mut builds = HashSet::new();
    let mut planned = HashSet::new();
    for document in snapshot.documents.values() {
        if let Some(build) = build_file(&document.path, &snapshot.roots, &sources)
            && builds.insert(build.clone())
        {
            let build_analysis =
                tooling::analyze_plan(&tooling::standalone_plan(&build), &sources, true);
            let valid = build_analysis
                .documents
                .values()
                .all(|d| !d.diagnostics.iter().any(|d| !d.warning));
            output.merge(build_analysis);
            planned.insert(build.clone());
            if valid {
                match PlanBuilder::from(build.clone())
                    .compile_and_interpret_build_file_with_sources(&sources)
                {
                    Ok(plan) => {
                        planned.extend(plan.files.iter().map(|f| f.path.clone()));
                        output.merge(tooling::analyze_plan(&plan, &sources, false));
                    }
                    Err(error) => output.add_error(&build, error, &sources),
                }
            }
        }
    }
    for document in snapshot.documents.values() {
        if !planned.contains(&document.path) {
            // While a project plan is invalid, lexical highlighting remains useful;
            // standalone semantics would manufacture false cross-file errors.
            if let Some(build) = build_file(&document.path, &snapshot.roots, &sources)
                && output
                    .documents
                    .get(&build)
                    .is_some_and(|d| d.diagnostics.iter().any(|d| !d.warning))
            {
                let tokens =
                    fibc::frontend::tokenizer::Tokenizer::new(&document.text).collect::<Vec<_>>();
                let semantic_tokens =
                    tooling::encode_tokens(&document.text, &tokens, &Default::default());
                output.documents.insert(
                    document.path.clone(),
                    tooling::DocumentAnalysis {
                        source: document.text.clone(),
                        tokens,
                        semantic_tokens,
                        diagnostics: vec![],
                    },
                );
                continue;
            }
            output.merge(tooling::analyze_plan(
                &tooling::standalone_plan(&document.path),
                &sources,
                false,
            ));
        }
    }
    output
}

struct Server {
    snapshot: Snapshot,
    initialized: bool,
    shutdown: bool,
    dynamic_watches: bool,
    result: Option<(u64, ProjectAnalysis)>,
    failure: Option<String>,
    pending: Vec<(Value, String)>,
    published: BTreeMap<String, Option<i64>>,
    jobs: mpsc::Sender<Snapshot>,
}
impl Server {
    fn schedule(&mut self) {
        self.snapshot.generation += 1;
        self.result = None;
        self.failure = None;
        let _ = self.jobs.send(self.snapshot.clone());
    }
    fn flush_tokens(&mut self) -> io::Result<()> {
        let Some((generation, result)) = &self.result else {
            return Ok(());
        };
        if *generation != self.snapshot.generation {
            return Ok(());
        }
        for (id, uri) in self.pending.drain(..) {
            if let Some(document) = self.snapshot.documents.get(&uri) {
                let data = result
                    .documents
                    .get(&document.path)
                    .map(|d| d.semantic_tokens.clone())
                    .unwrap_or_default();
                response(id, json!({"data":data}))?;
            } else {
                error(id, -32602, "document is not open")?;
            }
        }
        Ok(())
    }
    fn publish(&mut self, generation: u64, result: ProjectAnalysis) -> io::Result<()> {
        if generation != self.snapshot.generation || self.shutdown {
            return Ok(());
        }
        let mut published = BTreeMap::new();
        for (path, document) in &result.documents {
            let open = self.snapshot.documents.values().find(|d| d.path == *path);
            let uri = open
                .map(|d| d.uri.clone())
                .unwrap_or_else(|| path_uri(path));
            let version = open.map(|d| d.version);
            let index = SourceIndex::new(&document.source);
            let values = document.diagnostics.iter().map(|entry| {
                let d = &entry.diagnostic;
                let message = if let Some(hint) = &d.hint { format!("{}\nhelp: {hint}", d.message) } else { d.message.clone() };
                json!({"range":index.diagnostic_range(d, &document.tokens),"severity":if entry.warning {2} else {1},"code":d.kind.tag(),"source":"fibc","message":message})
            }).collect();
            diagnostics(&uri, version, values)?;
            published.insert(uri, version);
        }
        for uri in self.published.keys() {
            if !published.contains_key(uri) {
                diagnostics(
                    uri,
                    self.snapshot.documents.get(uri).map(|d| d.version),
                    vec![],
                )?;
            }
        }
        self.published = published;
        self.result = Some((generation, result));
        self.flush_tokens()
    }
    fn handle(&mut self, message: Value) -> io::Result<Option<i32>> {
        let id = message.get("id").cloned();
        if message.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            error(
                id.unwrap_or(Value::Null),
                -32600,
                "invalid JSON-RPC request",
            )?;
            return Ok(None);
        }
        if message.get("method").is_none()
            && (message.get("result").is_some() || message.get("error").is_some())
        {
            return Ok(None);
        }
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            error(id.unwrap_or(Value::Null), -32600, "missing method")?;
            return Ok(None);
        };
        let params = &message["params"];
        if method == "exit" {
            return Ok(Some(if self.shutdown { 0 } else { 1 }));
        }
        if self.shutdown {
            if let Some(id) = id {
                error(id, -32600, "server has shut down")?;
            }
            return Ok(None);
        }
        if method == "initialize" {
            if self.initialized {
                if let Some(id) = id {
                    error(id, -32600, "already initialized")?;
                }
                return Ok(None);
            }
            let folders = params["workspaceFolders"]
                .as_array()
                .map(|folders| {
                    folders
                        .iter()
                        .filter_map(|f| f["uri"].as_str().and_then(uri_path))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            self.snapshot.roots = if folders.is_empty() {
                params["rootUri"]
                    .as_str()
                    .and_then(uri_path)
                    .into_iter()
                    .collect()
            } else {
                folders
            };
            self.dynamic_watches =
                params["capabilities"]["workspace"]["didChangeWatchedFiles"]["dynamicRegistration"]
                    .as_bool()
                    .unwrap_or(false);
            self.initialized = true;
            response(
                id.unwrap_or(Value::Null),
                json!({"capabilities":{"positionEncoding":"utf-16","textDocumentSync":{"openClose":true,"change":1},"semanticTokensProvider":{"legend":{"tokenTypes":tooling::TOKEN_TYPES,"tokenModifiers":[]},"full":true}},"serverInfo":{"name":"fib-lsp","version":env!("CARGO_PKG_VERSION")}}),
            )?;
            return Ok(None);
        }
        if !self.initialized {
            if let Some(id) = id {
                error(id, -32002, "server is not initialized")?;
            }
            return Ok(None);
        }
        match method {
            "initialized" => {
                if self.dynamic_watches {
                    send(
                        json!({"jsonrpc":"2.0","id":"fib-watch-files","method":"client/registerCapability","params":{"registrations":[{"id":"fib-sources","method":"workspace/didChangeWatchedFiles","registerOptions":{"watchers":[{"globPattern":"**/*.fib"}]}}]}}),
                    )?;
                    self.dynamic_watches = false;
                }
            }
            "$/setTrace" => {}
            "shutdown" => {
                self.shutdown = true;
                self.pending.clear();
                response(id.unwrap_or(Value::Null), Value::Null)?;
            }
            "textDocument/didOpen" => {
                let value = &params["textDocument"];
                if let (Some(uri), Some(text), Some(version)) = (
                    value["uri"].as_str(),
                    value["text"].as_str(),
                    value["version"].as_i64(),
                ) && let Some(path) = uri_path(uri)
                {
                    self.snapshot.documents.insert(
                        uri.to_owned(),
                        Document {
                            uri: uri.to_owned(),
                            path,
                            text: text.to_owned(),
                            version,
                        },
                    );
                    self.schedule();
                }
            }
            "textDocument/didChange" => {
                if let (Some(uri), Some(version), Some(changes)) = (
                    params["textDocument"]["uri"].as_str(),
                    params["textDocument"]["version"].as_i64(),
                    params["contentChanges"].as_array(),
                ) && let Some(document) = self.snapshot.documents.get_mut(uri)
                    && version > document.version
                    && let Some(change) = changes.last()
                    && change.get("range").is_none()
                    && let Some(text) = change["text"].as_str()
                {
                    document.text = text.to_owned();
                    document.version = version;
                    self.schedule();
                }
            }
            "textDocument/didClose" => {
                if let Some(uri) = params["textDocument"]["uri"].as_str() {
                    self.snapshot.documents.remove(uri);
                    self.published.remove(uri);
                    diagnostics(uri, None, vec![])?;
                    self.schedule();
                }
            }
            "workspace/didChangeWatchedFiles" => self.schedule(),
            "textDocument/semanticTokens/full" => {
                if let Some(id) = id {
                    if let Some(message) = &self.failure {
                        error(id, -32603, message)?;
                        return Ok(None);
                    }
                    if let Some(uri) = params["textDocument"]["uri"]
                        .as_str()
                        .filter(|u| self.snapshot.documents.contains_key(*u))
                    {
                        self.pending.push((id, uri.to_owned()));
                        self.flush_tokens()?;
                    } else {
                        error(id, -32602, "document is not open")?;
                    }
                }
            }
            "$/cancelRequest" => {
                if let Some(index) = self.pending.iter().position(|(id, _)| *id == params["id"]) {
                    let (id, _) = self.pending.remove(index);
                    error(id, -32800, "request cancelled")?;
                }
            }
            _ => {
                if let Some(id) = id {
                    error(id, -32601, "method not found")?;
                }
            }
        }
        Ok(None)
    }
}

fn run() -> io::Result<i32> {
    let (events, receiver) = mpsc::channel();
    let input_events = events.clone();
    thread::spawn(move || {
        let mut input = io::BufReader::new(io::stdin());
        loop {
            match read_message(&mut input) {
                Ok(Some(message)) => {
                    if input_events.send(Event::Message(message)).is_err() {
                        break;
                    }
                }
                Ok(None) => {
                    let _ = input_events.send(Event::Eof);
                    break;
                }
                Err(error) => {
                    let _ = input_events.send(Event::Invalid(error.to_string()));
                    // Without trustworthy framing the stream cannot be resynchronized.
                    let _ = input_events.send(Event::Eof);
                    break;
                }
            }
        }
    });
    let (jobs, work) = mpsc::channel::<Snapshot>();
    thread::spawn(move || {
        while let Ok(mut snapshot) = work.recv() {
            for newer in work.try_iter() {
                snapshot = newer;
            }
            let generation = snapshot.generation;
            let result = std::panic::catch_unwind(|| analyze(snapshot));
            let event = match result {
                Ok(result) => Event::Analysis(generation, result),
                Err(_) => Event::Failure(generation, "frontend analysis panicked".into()),
            };
            if events.send(event).is_err() {
                break;
            }
        }
    });
    let mut server = Server {
        snapshot: Snapshot {
            generation: 0,
            documents: BTreeMap::new(),
            roots: vec![],
        },
        initialized: false,
        shutdown: false,
        dynamic_watches: false,
        result: None,
        failure: None,
        pending: vec![],
        published: BTreeMap::new(),
        jobs,
    };
    while let Ok(event) = receiver.recv() {
        match event {
            Event::Message(message) => {
                if let Some(code) = server.handle(message)? {
                    return Ok(code);
                }
            }
            Event::Invalid(message) => error(Value::Null, -32700, &message)?,
            Event::Eof => return Ok(if server.shutdown { 0 } else { 1 }),
            Event::Analysis(generation, result) => server.publish(generation, result)?,
            Event::Failure(generation, message) if generation == server.snapshot.generation => {
                eprintln!("fib-lsp: {message}");
                server.failure = Some(message.clone());
                send(
                    json!({"jsonrpc":"2.0","method":"window/logMessage","params":{"type":1,"message":message}}),
                )?;
                for (id, _) in server.pending.drain(..) {
                    error(id, -32603, &message)?;
                }
            }
            Event::Failure(_, _) => {}
        }
    }
    Ok(1)
}
fn main() {
    std::process::exit(match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("fib-lsp: {error}");
            1
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_concatenated_frames_and_rejects_invalid_lengths() {
        let body = json!({"jsonrpc":"2.0","id":1,"method":"initialize"}).to_string();
        let frame = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
        let mut input = io::Cursor::new(format!("{frame}{frame}"));
        assert_eq!(read_message(&mut input).unwrap().unwrap()["id"], 1);
        assert_eq!(read_message(&mut input).unwrap().unwrap()["id"], 1);
        assert!(read_message(&mut input).unwrap().is_none());
        assert!(read_message(&mut io::Cursor::new("Content-Length: 999999999\r\n\r\n")).is_err());
    }
    #[test]
    fn file_uris_round_trip_unicode_spaces_and_reserved_characters() {
        let path = PathBuf::from("/tmp/fib 😀 +#%.fib");
        assert_eq!(uri_path(&path_uri(&path)), Some(path));
        assert!(uri_path("https://example.com/file.fib").is_none());
        assert!(uri_path("file://remote/file.fib").is_none());
        assert!(uri_path("file:///tmp/%xx").is_none());
    }
    #[test]
    fn workspace_boundary_prevents_loading_an_outside_build_file() {
        let root = tempfile::tempdir().unwrap();
        let workspace = root.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        std::fs::write(root.path().join("build.fib"), "").unwrap();
        assert!(
            build_file(
                &workspace.join("a.fib"),
                std::slice::from_ref(&workspace),
                &OverlaySources::default()
            )
            .is_none()
        );
    }
    #[test]
    fn stale_results_cannot_update_cache_or_publish_diagnostics() {
        let (jobs, _) = mpsc::channel();
        let mut server = Server {
            snapshot: Snapshot {
                generation: 3,
                documents: BTreeMap::new(),
                roots: vec![],
            },
            initialized: true,
            shutdown: false,
            dynamic_watches: false,
            result: None,
            failure: None,
            pending: vec![],
            published: BTreeMap::new(),
            jobs,
        };
        server.publish(2, ProjectAnalysis::default()).unwrap();
        assert!(server.result.is_none());
        assert!(server.published.is_empty());
    }
}
