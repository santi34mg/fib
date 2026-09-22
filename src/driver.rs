use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::{fs, io};

#[cfg(feature = "llvm")]
use crate::backend::lowering;
use crate::diagnostics::{CompilerError, ErrorKind, source_line_at};
use crate::frontend::analyze::{AnalysisError, analyze};
use crate::frontend::ast::{Ast, declaration::DeclarationNode};
use crate::frontend::identifier::Identifier;
use crate::frontend::parser::ParseError;
use crate::frontend::parser::Parser;
use crate::frontend::typed_ast::{TypedDecl, TypedModule, TypedProgram};
use crate::frontend::{lexer::Lexer, tokens::Token};

// ---------------------------------------------------------------------------
// Emit kind
// ---------------------------------------------------------------------------

/// Which compilation stage to stop after.
///
/// - `Lex` prints tokens, `Parse` prints the AST, `Typed` prints the typed
///   program. None of these need the LLVM backend.
/// - `Llvm` writes LLVM IR to a `.ll` file.
/// - `Bin` (default) links a native binary, optionally keeping the `.ll`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EmitKind {
    Lex,
    Parse,
    Typed,
    Llvm,
    #[default]
    Bin,
}

impl fmt::Display for EmitKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            EmitKind::Lex => "lex",
            EmitKind::Parse => "parse",
            EmitKind::Typed => "typed",
            EmitKind::Llvm => "llvm",
            EmitKind::Bin => "bin",
        };
        write!(f, "{}", s)
    }
}

impl std::str::FromStr for EmitKind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "lex" => Ok(EmitKind::Lex),
            "parse" => Ok(EmitKind::Parse),
            "typed" => Ok(EmitKind::Typed),
            "llvm" => Ok(EmitKind::Llvm),
            "bin" => Ok(EmitKind::Bin),
            other => Err(format!(
                "invalid --emit '{}': expected lex|parse|typed|llvm|bin",
                other
            )),
        }
    }
}

impl EmitKind {
    /// True for stages that need the LLVM backend (`llvm` + `bin`).
    pub fn needs_backend(&self) -> bool {
        matches!(self, EmitKind::Llvm | EmitKind::Bin)
    }
}

// ---------------------------------------------------------------------------
// Compilation options
// ---------------------------------------------------------------------------

/// Owned compilation options. Construct directly (library) or via
/// `CompilationOptions::from(cli::Args)` (binary). All paths are owned so
/// there are no lifetime parameters.
#[derive(Debug, Clone)]
pub struct CompilationOptions {
    /// Entry `.fib` file.
    pub project_path: PathBuf,
    /// In-memory source override. `None` (the common case) reads from
    /// `project_path`. `Some` enables unit tests without touching disk.
    pub source_override: Option<String>,
    /// Extra directories searched when resolving imports, in addition to the
    /// entry file's directory.
    pub include_paths: Vec<PathBuf>,
    /// Output binary path for `--emit=bin`. Defaults to `out/<stem>`.
    /// When `--emit=llvm`, this (if set) is the `.ll` output path.
    pub output: Option<PathBuf>,
    /// Stage to stop after.
    pub emit: EmitKind,
    /// `--check`: run the frontend only, no codegen. Overrides `emit`.
    pub check_only: bool,
    /// Keep the intermediate `.ll` file for `--emit=bin`.
    pub emit_llvm: bool,
    /// Explicit `.ll` output path (implies keeping it).
    pub llvm_out: Option<PathBuf>,
    /// C compiler used for linking. Defaults to `$CC`, else `clang-17`, else
    /// `clang`.
    pub cc: Option<PathBuf>,
    /// Optimization level passed to clang as `-O<level>`.
    /// Accepted: `0`, `1`, `2`, `3`, `s`, `z`.
    pub opt_level: Option<String>,
}

impl CompilationOptions {
    pub fn new(project_path: PathBuf) -> Self {
        Self {
            project_path,
            source_override: None,
            include_paths: Vec::new(),
            output: None,
            emit: EmitKind::Bin,
            check_only: false,
            emit_llvm: false,
            llvm_out: None,
            cc: None,
            opt_level: None,
        }
    }

    /// The stage that will actually run (`--check` forces `Typed`).
    pub fn effective_emit(&self) -> EmitKind {
        if self.check_only {
            EmitKind::Typed
        } else {
            self.emit
        }
    }

    /// `[entry_dir] + include_paths`, in search order.
    pub fn search_roots(&self, src_root: &Path) -> Vec<PathBuf> {
        let mut roots = vec![src_root.to_path_buf()];
        roots.extend(self.include_paths.iter().cloned());
        roots
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Structured driver error. `Parse` / `Analysis` preserve the underlying
/// diagnostic (including `file:line:col`) instead of collapsing it to a
/// generic string.
#[derive(Debug)]
pub enum DriverError {
    NotAFile(PathBuf),
    NotFibFile(PathBuf),
    Io {
        path: PathBuf,
        source: io::Error,
    },
    Parse(ParseError),
    ImportNotFound {
        module: String,
        searched: Vec<PathBuf>,
    },
    CircularImport {
        stack: Vec<String>,
    },
    ImportRead {
        module: String,
        path: PathBuf,
        source: io::Error,
    },
    Analysis(AnalysisError),
    ModuleAnalysis {
        module: String,
        source: AnalysisError,
    },
    Lower(String),
    Emit {
        path: PathBuf,
        reason: String,
    },
    Link {
        status: String,
        stdout: String,
        stderr: String,
    },
    CcNotFound {
        attempted: Vec<String>,
    },
    InvalidOptLevel(String),
    LlvmUnavailable(&'static str),
    EmptyImportPath,
}

impl fmt::Display for DriverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DriverError::NotAFile(p) => write!(f, "Not a file: {:?}", p),
            DriverError::NotFibFile(p) => write!(f, "Not a fib file: {:?}", p),
            DriverError::Io { path, source } => {
                write!(f, "Failed to read {}: {}", path.display(), source)
            }
            // ParseError's Display already renders `file:line:col` + message.
            DriverError::Parse(e) => write!(f, "{}", e),
            DriverError::ImportNotFound { module, searched } => {
                write!(
                    f,
                    "cannot read module '{}': No such file or directory",
                    module
                )?;
                if !searched.is_empty() {
                    write!(f, " (searched: ")?;
                    for (i, r) in searched.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{}", r.display())?;
                    }
                    write!(f, ")")?;
                }
                Ok(())
            }
            DriverError::CircularImport { stack } => {
                write!(f, "circular import detected: {}", stack.join(" -> "))
            }
            DriverError::ImportRead {
                module,
                path,
                source,
            } => write!(
                f,
                "cannot read module '{}' ({}): {}",
                module,
                path.display(),
                source
            ),
            DriverError::Analysis(e) => write!(f, "Analysis failed: {}", e),
            DriverError::ModuleAnalysis { module, source } => {
                write!(f, "Analysis failed in module '{}': {}", module, source)
            }
            DriverError::Lower(msg) => write!(f, "Lowering failed: {}", msg),
            DriverError::Emit { path, reason } => {
                write!(f, "Failed to write {}: {}", path.display(), reason)
            }
            DriverError::Link {
                status,
                stdout,
                stderr,
            } => {
                write!(f, "clang failed with status {}", status)?;
                if !stdout.trim().is_empty() {
                    write!(f, "\n--- stdout ---\n{}", stdout)?;
                }
                if !stderr.trim().is_empty() {
                    write!(f, "\n--- stderr ---\n{}", stderr)?;
                }
                Ok(())
            }
            DriverError::CcNotFound { attempted } => {
                write!(f, "C compiler not found (tried: {})", attempted.join(", "))
            }
            DriverError::InvalidOptLevel(lvl) => write!(
                f,
                "Invalid opt level '{}': expected one of 0,1,2,3,s,z",
                lvl
            ),
            DriverError::LlvmUnavailable(what) => write!(
                f,
                "{} requires the `llvm` cargo feature (rebuild with --features llvm)",
                what
            ),
            DriverError::EmptyImportPath => write!(f, "empty import path"),
        }
    }
}

impl std::error::Error for DriverError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DriverError::Parse(e) => Some(e),
            DriverError::Analysis(e) => Some(e),
            DriverError::ModuleAnalysis { source, .. } => Some(source),
            DriverError::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<ParseError> for DriverError {
    fn from(value: ParseError) -> Self {
        DriverError::Parse(value)
    }
}

impl From<AnalysisError> for DriverError {
    fn from(value: AnalysisError) -> Self {
        DriverError::Analysis(value)
    }
}

impl From<DriverError> for CompilerError {
    fn from(e: DriverError) -> Self {
        match e {
            DriverError::Parse(pe) => pe.into(),
            DriverError::Analysis(ae) => ae.into(),
            DriverError::ModuleAnalysis { module, source } => {
                let mut ce = CompilerError::from(source);
                ce.filename = Some(PathBuf::from(module));
                ce
            }
            DriverError::NotAFile(p) => {
                CompilerError::new(ErrorKind::Io, format!("not a file: {}", p.display()))
            }
            DriverError::NotFibFile(p) => {
                CompilerError::new(ErrorKind::Io, format!("not a fib file: {}", p.display()))
            }
            DriverError::Io { path, source } => CompilerError::new(
                ErrorKind::Io,
                format!("failed to read {}: {}", path.display(), source),
            ),
            DriverError::ImportNotFound { module, searched } => {
                let mut msg = format!("cannot read module '{}': no such file or directory", module);
                if !searched.is_empty() {
                    let roots = searched
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ");
                    msg.push_str(&format!(" (searched: {})", roots));
                }
                CompilerError::new(ErrorKind::Name, msg)
            }
            DriverError::CircularImport { stack } => CompilerError::new(
                ErrorKind::Name,
                format!("circular import detected: {}", stack.join(" -> ")),
            ),
            DriverError::ImportRead {
                module,
                path,
                source,
            } => CompilerError::new(
                ErrorKind::Io,
                format!(
                    "cannot read module '{}' ({}): {}",
                    module,
                    path.display(),
                    source
                ),
            ),
            DriverError::Lower(msg) => {
                CompilerError::new(ErrorKind::Backend, format!("lowering failed: {}", msg))
            }
            DriverError::Emit { path, reason } => CompilerError::new(
                ErrorKind::Io,
                format!("failed to write {}: {}", path.display(), reason),
            ),
            DriverError::Link {
                status,
                stdout,
                stderr,
            } => {
                let mut msg = format!("clang failed with status {}", status);
                if !stdout.trim().is_empty() {
                    msg.push_str(&format!("\n--- stdout ---\n{}", stdout));
                }
                if !stderr.trim().is_empty() {
                    msg.push_str(&format!("\n--- stderr ---\n{}", stderr));
                }
                CompilerError::new(ErrorKind::Toolchain, msg)
            }
            DriverError::CcNotFound { attempted } => CompilerError::new(
                ErrorKind::Toolchain,
                format!("C compiler not found (tried: {})", attempted.join(", ")),
            ),
            DriverError::InvalidOptLevel(lvl) => CompilerError::new(
                ErrorKind::Toolchain,
                format!("invalid opt level '{}': expected one of 0,1,2,3,s,z", lvl),
            ),
            DriverError::LlvmUnavailable(what) => CompilerError::new(
                ErrorKind::Toolchain,
                format!(
                    "{} requires the `llvm` cargo feature (rebuild with --features llvm)",
                    what
                ),
            ),
            DriverError::EmptyImportPath => {
                CompilerError::new(ErrorKind::Io, "empty import path".to_string())
            }
        }
    }
}

fn io_error(path: &Path, source: io::Error) -> DriverError {
    DriverError::Io {
        path: path.to_path_buf(),
        source,
    }
}

// ---------------------------------------------------------------------------
// Frontend outputs
// ---------------------------------------------------------------------------

/// Successful frontend result: lex + parse + analyze, with imports resolved.
/// This is what `--check` / `--emit=lex|parse|typed` inspect.
#[derive(Debug)]
pub struct FrontendResponse {
    pub tokens: Vec<Token>,
    pub ast: Ast,
    pub typed_program: TypedProgram,
    pub resolved_modules: HashMap<Vec<String>, TypedModule>,
    /// Entry source text (from disk or `source_override`).
    pub source: String,
    /// Entry filename as passed to the parser (for diagnostics).
    pub filename: String,
}

/// What `compile()` produced. The CLI prints a short summary; library users
/// inspect the paths directly instead of parsing stdout.
#[derive(Debug)]
pub enum CompileOutput {
    /// `--emit=lex|parse|typed` or `--check`: no files written.
    Frontend {
        emit: EmitKind,
        frontend: Box<FrontendResponse>,
    },
    /// `--emit=llvm`: only the `.ll` file was written.
    Llvm { path: PathBuf },
    /// `--emit=bin` (default): native binary, plus the `.ll` iff kept.
    Binary {
        binary: PathBuf,
        llvm_ir: Option<PathBuf>,
    },
}

// ---------------------------------------------------------------------------
// Stage 1: validate + read
// ---------------------------------------------------------------------------

/// Check the entry path exists and has a `.fib` extension.
///
/// When `source_override` is set (in-memory tests) the existence check is
/// skipped but the extension check still applies so error messages stay
/// consistent.
pub fn validate_path(path: &Path, has_source_override: bool) -> Result<(), DriverError> {
    if path.extension().and_then(|s| s.to_str()) != Some("fib") {
        return Err(DriverError::NotFibFile(path.to_path_buf()));
    }
    if !has_source_override && !path.is_file() {
        return Err(DriverError::NotAFile(path.to_path_buf()));
    }
    Ok(())
}

/// Read the entry source, honoring `source_override` for tests.
pub fn read_source(path: &Path, source_override: Option<&str>) -> Result<String, DriverError> {
    if let Some(src) = source_override {
        return Ok(src.to_string());
    }
    fs::read_to_string(path).map_err(|e| io_error(path, e))
}

// ---------------------------------------------------------------------------
// Stage 2: lex + parse
// ---------------------------------------------------------------------------

pub fn lex_source(source: &str) -> Vec<Token> {
    Lexer::new(source).collect()
}

pub fn parse_source(tokens: Vec<Token>, file: &Path, source: &str) -> Result<Ast, DriverError> {
    let mut parser = Parser::new(tokens.into_iter(), file, source.to_string());
    parser.parse().map_err(DriverError::from)
}

// ---------------------------------------------------------------------------
// Stage 3: module resolution
// ---------------------------------------------------------------------------

/// Import paths declared by `ast`, e.g. `import std::io` -> `["std", "io"]`.
fn entry_imports(ast: &Ast) -> Result<Vec<Vec<String>>, DriverError> {
    let mut out = Vec::new();
    for decl in &ast.declarations {
        if let DeclarationNode::ImportDeclaration(import_decl) = decl {
            if import_decl.path.is_empty() {
                return Err(DriverError::EmptyImportPath);
            }
            out.push(import_decl.path.iter().map(|id| id.value.clone()).collect());
        }
    }
    Ok(out)
}

/// Candidate file paths for an import, in probe order.
///
/// Pure (no I/O): one full-path candidate per root, each immediately followed
/// by its drop-first-segment fallback. Both `load_module_source` (content
/// probe) and `resolve_module_path` (metadata probe) iterate this list in
/// order, so they always agree on which file an import means.
///
/// The two-way search is intentionally kept (2026-09-18): for each root we
/// try the full path (`["std","libc"]` -> `<root>/std/libc.fib`) and then the
/// fallback with the first segment dropped (`<root>/libc.fib`). The fallback
/// is load-bearing — samples `import std::libc` with `-I <repo>/std` only
/// resolve via `<std>/libc.fib` — so removing it would break that layout.
/// Canonical-path interning in `resolve_module_recursive` guarantees both
/// spellings of the same file share one `TypedModule` instead of
/// duplicating it.
fn module_candidate_paths(import_path: &[String], search_roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for root in search_roots {
        // 1. Full path: <root>/a/b.fib
        let mut full = root.clone();
        for segment in import_path {
            full.push(segment);
        }
        full.set_extension("fib");
        out.push(full);
        // 2. Drop-first-segment fallback: <root>/b.fib
        if import_path.len() > 1 {
            let mut short = root.clone();
            for segment in &import_path[1..] {
                short.push(segment);
            }
            short.set_extension("fib");
            out.push(short);
        }
    }
    out
}

/// Locate an import on disk without parsing it.
///
/// Tries each `search_roots` entry in order (see `module_candidate_paths`
/// for the retained two-way search).
pub fn load_module_source(
    import_path: &[String],
    search_roots: &[PathBuf],
) -> Result<(PathBuf, String), DriverError> {
    let module = import_path.join("::");
    let mut attempted_file: Option<(PathBuf, io::Error)> = None;
    for candidate in module_candidate_paths(import_path, search_roots) {
        match fs::read_to_string(&candidate) {
            Ok(src) => return Ok((candidate, src)),
            Err(e) => {
                if attempted_file.is_none() {
                    attempted_file = Some((candidate, e));
                }
            }
        }
    }
    if let Some((path, source)) = attempted_file
        && source.kind() != io::ErrorKind::NotFound
    {
        return Err(DriverError::ImportRead {
            module,
            path,
            source,
        });
    }
    Err(DriverError::ImportNotFound {
        module,
        searched: search_roots.to_vec(),
    })
}

/// Resolve an import to a file path without reading it.
///
/// Metadata-only probe over the same candidates as `load_module_source`, in
/// the same order, so both agree on which file an import means. Used for the
/// pre-parse canonical-path cache probe in `resolve_module_recursive`; the
/// authoritative read (and its `ImportRead` vs `ImportNotFound` error) still
/// comes from `load_module` below, so probe misses here are harmless and
/// produce no error on their own.
fn resolve_module_path(
    import_path: &[String],
    search_roots: &[PathBuf],
) -> Result<PathBuf, DriverError> {
    let module = import_path.join("::");
    let mut attempted: Option<(PathBuf, io::Error)> = None;
    for candidate in module_candidate_paths(import_path, search_roots) {
        match fs::metadata(&candidate) {
            Ok(md) if md.is_file() => return Ok(candidate),
            // A directory (or other non-file) named `x.fib`: keep looking.
            Ok(_) => continue,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => {
                if attempted.is_none() {
                    attempted = Some((candidate, e));
                }
            }
        }
    }
    if let Some((path, source)) = attempted {
        return Err(DriverError::ImportRead {
            module,
            path,
            source,
        });
    }
    Err(DriverError::ImportNotFound {
        module,
        searched: search_roots.to_vec(),
    })
}

/// Parse an already-loaded module file. The parser embeds `path` in errors,
/// so no extra context is needed here.
fn parse_module_ast(path: &Path, source: &str) -> Result<Ast, DriverError> {
    let tokens: Vec<Token> = lex_source(source);
    parse_source(tokens, path, source)
}

/// Load + parse one module: the single resolve-read-parse entry point for
/// imports. `resolve_module_recursive` routes every transitive import through
/// here; the entry file goes through `load_entry`, and both funnel through
/// the shared `lex_source`/`parse_source` core so there is exactly one
/// lex+parse path.
pub fn load_module(
    import_path: &[String],
    search_roots: &[PathBuf],
) -> Result<(PathBuf, String, Ast), DriverError> {
    let (file_path, source) = load_module_source(import_path, search_roots)?;
    let ast = parse_module_ast(&file_path, &source)?;
    Ok((file_path, source, ast))
}

/// Load the entry file: `read_source` (honoring `source_override` for tests)
/// plus the shared lex+parse core. Returns the tokens too, since
/// `FrontendResponse` carries them.
fn load_entry(
    path: &Path,
    source_override: Option<&str>,
) -> Result<(String, Vec<Token>, Ast), DriverError> {
    let source = read_source(path, source_override)?;
    let tokens = lex_source(&source);
    let ast = parse_source(tokens.clone(), path, &source)?;
    Ok((source, tokens, ast))
}

/// Canonicalize a resolved module file into a stable cache key.
///
/// `fs::canonicalize` resolves symlinks and `.`/`..` so the same file reached
/// via different import spellings (full path vs drop-first-segment fallback)
/// maps to one key. This is the path-interning step: one canonical `PathBuf`
/// per distinct file, shared via the `path_intern` map in `resolve_imports`
/// (a full string-interning arena / incremental cache is deliberately out of
/// scope — see the module docs on the short-term slice).
/// Falls back to a lexical absolute path when canonicalization fails (e.g. a
/// TOCTOU deletion between probe and load) so caching degrades to
/// per-spelling keys instead of panicking.
fn canonicalize_module_path(path: &Path) -> PathBuf {
    if let Ok(canonical) = fs::canonicalize(path) {
        return canonical;
    }
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    let mut out = PathBuf::new();
    for component in abs.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Resolve every (transitive) import of the entry AST.
///
/// Returns a map from import path (`["std","libc"]`) to typed module.
/// Uses a `HashSet` worklist for cycle detection and reports the full import
/// stack (`a -> b -> a`) instead of just the offending module.
///
/// Short-term module cache (deliberately not visibility / separate
/// compilation / incremental — that redesign is out of scope):
/// - `resolved` stays keyed by logical import path (`Vec<String>`): that is
///   the key `analyze()` looks up, so the outward shape is unchanged.
/// - Internally each loaded file is canonicalized
///   (`canonicalize_module_path`, symlinks + `.`/`..` resolved) and interned
///   in `path_intern` (one canonical `PathBuf` per distinct file). A second
///   import spelling that hits the same file reuses the cached `TypedModule`
///   without re-reading, re-parsing, or re-analyzing — this is the
///   `Ast`/`TypedModule` cache keyed by resolved path. Canonical paths of
///   in-progress modules are tracked in `resolving_paths` so an alias-spelled
///   cycle is still reported as `CircularImport`.
/// - Parse errors are not cached: the first failure aborts resolution, so a
///   broken file is never parsed twice in one compilation. File context is
///   preserved structurally — the parser embeds the file path in every
///   `ParseError` surfaced as `DriverError::Parse`.
pub fn resolve_imports(
    entry_ast: &Ast,
    search_roots: &[PathBuf],
) -> Result<HashMap<Vec<String>, TypedModule>, DriverError> {
    let mut resolved: HashMap<Vec<String>, TypedModule> = HashMap::new();
    let mut resolving_set: HashSet<Vec<String>> = HashSet::new();
    let mut resolving_stack: Vec<Vec<String>> = Vec::new();
    let mut path_intern: HashMap<PathBuf, Vec<String>> = HashMap::new();
    let mut resolving_paths: HashSet<PathBuf> = HashSet::new();
    for import_path in entry_imports(entry_ast)? {
        if !resolved.contains_key(&import_path) {
            resolve_module_recursive(
                &import_path,
                search_roots,
                &mut resolved,
                &mut resolving_set,
                &mut resolving_stack,
                &mut path_intern,
                &mut resolving_paths,
            )?;
        }
    }
    Ok(resolved)
}

fn resolve_module_recursive(
    path: &[String],
    search_roots: &[PathBuf],
    resolved: &mut HashMap<Vec<String>, TypedModule>,
    resolving_set: &mut HashSet<Vec<String>>,
    resolving_stack: &mut Vec<Vec<String>>,
    path_intern: &mut HashMap<PathBuf, Vec<String>>,
    resolving_paths: &mut HashSet<PathBuf>,
) -> Result<(), DriverError> {
    if resolved.contains_key(path) {
        return Ok(());
    }
    if resolving_set.contains(path) {
        let mut stack: Vec<String> = resolving_stack.iter().map(|p| p.join("::")).collect();
        stack.push(path.join("::"));
        return Err(DriverError::CircularImport { stack });
    }
    // Canonical-path probe (metadata only, no content read): alias + cycle
    // detection across different import spellings of the same file. Probe
    // failures are ignored — `load_module` below reports the authoritative
    // `ImportNotFound` / `ImportRead`.
    let probed_canonical: Option<PathBuf> = resolve_module_path(path, search_roots)
        .ok()
        .map(|p| canonicalize_module_path(&p));
    if let Some(canon) = &probed_canonical {
        if let Some(first_key) = path_intern.get(canon)
            && let Some(first_module) = resolved.get(first_key)
        {
            resolved.insert(path.to_vec(), first_module.clone());
            return Ok(());
        }
        if resolving_paths.contains(canon) {
            let mut stack: Vec<String> = resolving_stack.iter().map(|p| p.join("::")).collect();
            stack.push(path.join("::"));
            return Err(DriverError::CircularImport { stack });
        }
    }
    resolving_set.insert(path.to_vec());
    resolving_stack.push(path.to_vec());
    if let Some(canon) = &probed_canonical {
        resolving_paths.insert(canon.clone());
    }

    let result = (|| {
        let (file_path, _source, ast) = load_module(path, search_roots)?;
        let canonical = canonicalize_module_path(&file_path);

        for decl in &ast.declarations {
            if let DeclarationNode::ImportDeclaration(import) = decl {
                if import.path.is_empty() {
                    return Err(DriverError::EmptyImportPath);
                }
                let nested: Vec<String> = import.path.iter().map(|id| id.value.clone()).collect();
                if !resolved.contains_key(&nested) {
                    resolve_module_recursive(
                        &nested,
                        search_roots,
                        resolved,
                        resolving_set,
                        resolving_stack,
                        path_intern,
                        resolving_paths,
                    )?;
                }
            }
        }

        let cu = analyze(ast, resolved).map_err(|e| DriverError::ModuleAnalysis {
            module: path.join("::"),
            source: e,
        })?;
        let module_name = path.last().cloned().unwrap_or_default();
        let module = TypedModule {
            name: module_name,
            path: path
                .iter()
                .map(|s| Identifier { value: s.clone() })
                .collect(),
            exports: cu.symbol_table.global_symbols().clone(),
            declarations: [cu.declarations, cu.imported_declarations].concat(),
        };
        resolved.insert(path.to_vec(), module);
        path_intern.insert(canonical, path.to_vec());
        Ok(())
    })();

    resolving_set.remove(path);
    resolving_stack.pop();
    if let Some(canon) = &probed_canonical {
        resolving_paths.remove(canon);
    }
    result
}

// ---------------------------------------------------------------------------
// Stage 4: analyze + merge
// ---------------------------------------------------------------------------

pub fn analyze_entry(
    ast: Ast,
    resolved: &HashMap<Vec<String>, TypedModule>,
) -> Result<TypedProgram, DriverError> {
    analyze(ast, resolved).map_err(DriverError::from)
}

fn decl_key(decl: &TypedDecl) -> String {
    match decl {
        TypedDecl::Function(f) => format!("fn:{}", f.name.value),
        TypedDecl::Type(t) => format!("type:{}", t.name.value),
        TypedDecl::Const(c) => format!("const:{}", c.name.value),
    }
}

/// Merge `imported` + `local` declarations, deduplicating diamond imports.
///
/// The same module can be reached through several import paths, so its
/// declarations would otherwise be emitted twice (previously papered over by
/// a skip-if-has-body hack in LLVM lowering). Last occurrence wins so entry
/// (local) declarations shadow imported ones with the same name; ordering
/// follows last occurrence for determinism.
pub fn dedupe_declarations(imported: Vec<TypedDecl>, local: Vec<TypedDecl>) -> Vec<TypedDecl> {
    let mut order: Vec<String> = Vec::new();
    let mut map: HashMap<String, TypedDecl> = HashMap::new();
    for decl in imported.into_iter().chain(local) {
        let key = decl_key(&decl);
        if map.contains_key(&key) {
            order.retain(|k| k != &key);
        }
        order.push(key.clone());
        map.insert(key, decl);
    }
    order.into_iter().filter_map(|k| map.remove(&k)).collect()
}

/// Run lex + parse + import resolution + analysis. No LLVM involved, so this
/// works with `--no-default-features` and is unit-testable without clang.
pub fn run_frontend(opts: &CompilationOptions) -> Result<FrontendResponse, DriverError> {
    let file = &opts.project_path;
    validate_path(file, opts.source_override.is_some())?;
    let (source, tokens, ast) = load_entry(file, opts.source_override.as_deref())?;
    let filename = file.to_string_lossy().to_string();
    let src_root = match file.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    let search_roots = opts.search_roots(src_root);

    let resolved = resolve_imports(&ast, &search_roots)?;
    let typed_program = analyze_entry(ast.clone(), &resolved)?;

    Ok(FrontendResponse {
        tokens,
        ast,
        typed_program,
        resolved_modules: resolved,
        source,
        filename,
    })
}

/// Frontend-only entry point for `--check` and library users.
pub fn check_project(opts: &CompilationOptions) -> Result<FrontendResponse, DriverError> {
    run_frontend(opts)
}

// ---------------------------------------------------------------------------
// Stage 5: lower (LLVM-gated)
// ---------------------------------------------------------------------------

#[cfg(feature = "llvm")]
pub fn lower_to_llvm_ir(program: TypedProgram, module_name: &str) -> Result<String, DriverError> {
    // Middle-end first: try the flat `IrProgram` path (`lower_ir`). It
    // handles locals, integer/bool/float arithmetic, calls, `if`/`for`/
    // `break`/`continue`, inline `defer`, `return`, assignment forms,
    // plain-enum `switch`, and string literals. Anything outside that core
    // subset (structs, tuples, pointers, enum payloads, referenced
    // module-level `const` globals, multi-value returns) is rejected with
    // `LowerError::Unsupported` by the importer or the consumer, so we fall
    // back to the direct `TypedProgram -> LLVM` path. Whichever path runs,
    // the emitted IR is correct; which one is live is observable per program.
    if let Ok(ir_program) = crate::ir::lower_typed_program(program.clone())
        && let Ok(text) = crate::backend::lowering::lower_ir(ir_program, module_name)
    {
        return Ok(text);
    }
    lowering::lower(program, module_name).map_err(|e| DriverError::Lower(e.to_string()))
}

#[cfg(not(feature = "llvm"))]
pub fn lower_to_llvm_ir(_program: TypedProgram, _module_name: &str) -> Result<String, DriverError> {
    Err(DriverError::LlvmUnavailable("codegen"))
}

// ---------------------------------------------------------------------------
// Stage 6: emit + link
// ---------------------------------------------------------------------------

/// Default binary output: `out/<stem>`.
pub fn default_binary_path(input: &Path) -> PathBuf {
    let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("a");
    PathBuf::from("out").join(stem)
}

/// Default LLVM IR output: `out/<stem>.ll`.
pub fn default_llvm_path(input: &Path) -> PathBuf {
    let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("a");
    PathBuf::from("out").join(format!("{}.ll", stem))
}

/// Binary path for `--emit=bin`: `--output` or `out/<stem>`.
pub fn binary_output_path(opts: &CompilationOptions, input: &Path) -> PathBuf {
    opts.output
        .clone()
        .unwrap_or_else(|| default_binary_path(input))
}

/// LLVM IR path, or `None` when a temp file should be used.
///
/// - `--emit=llvm`: `--llvm-out` > `--output` > `out/<stem>.ll`.
/// - `--emit=bin` with `--llvm-out`/`--emit-llvm`: that path, else the binary
///   path with an `.ll` extension.
/// - Otherwise (`--emit=bin` without keep flags): `None` (temp file).
pub fn llvm_output_path(
    opts: &CompilationOptions,
    emit: EmitKind,
    binary_path: &Path,
    input: &Path,
) -> Option<PathBuf> {
    match emit {
        EmitKind::Llvm => Some(
            opts.llvm_out
                .clone()
                .or_else(|| opts.output.clone())
                .unwrap_or_else(|| default_llvm_path(input)),
        ),
        EmitKind::Bin => {
            if let Some(p) = &opts.llvm_out {
                Some(p.clone())
            } else if opts.emit_llvm {
                Some(binary_path.with_extension("ll"))
            } else {
                None
            }
        }
        _ => None,
    }
}

pub fn emit_llvm_ir(ir: &str, path: &Path) -> Result<(), DriverError> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|e| DriverError::Emit {
            path: parent.to_path_buf(),
            reason: e.to_string(),
        })?;
    }
    fs::write(path, ir).map_err(|e| DriverError::Emit {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })
}

fn validate_opt_level(level: &str) -> Result<String, DriverError> {
    match level {
        "0" | "1" | "2" | "3" | "s" | "z" => Ok(format!("-O{}", level)),
        other => Err(DriverError::InvalidOptLevel(other.to_string())),
    }
}

/// Candidate C compilers in priority order: `--cc` > `$CC` > `clang-17` >
/// `clang`.
pub fn cc_candidates(explicit: Option<&Path>) -> Vec<PathBuf> {
    if let Some(cc) = explicit {
        return vec![cc.to_path_buf()];
    }
    if let Ok(env_cc) = std::env::var("CC")
        && !env_cc.trim().is_empty()
    {
        // `$CC` may be "clang" or an absolute path; only the program name is
        // used (extra flags belong in CFLAGS, not CC).
        let prog = env_cc.split_whitespace().next().unwrap_or("clang");
        return vec![PathBuf::from(prog)];
    }
    vec![PathBuf::from("clang-17"), PathBuf::from("clang")]
}

/// Link LLVM IR into a native binary via clang. Reports `stdout`/`stderr`
/// separately (see audit 03 §2) and falls back from `clang-17` to `clang`
/// when no explicit `--cc`/`$CC` is set.
pub fn link_llvm_ir(
    ll_path: &Path,
    bin_path: &Path,
    cc: Option<&Path>,
    opt_level: Option<&str>,
) -> Result<(), DriverError> {
    let opt_flag = opt_level
        .map(validate_opt_level)
        .transpose()?
        .map(Some)
        .unwrap_or(None);

    if let Some(parent) = bin_path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|e| DriverError::Emit {
            path: parent.to_path_buf(),
            reason: e.to_string(),
        })?;
    }

    let candidates = cc_candidates(cc);
    let mut last_not_found: Option<io::Error> = None;
    for (i, candidate) in candidates.iter().enumerate() {
        let mut cmd = std::process::Command::new(candidate);
        cmd.arg(ll_path).arg("-o").arg(bin_path);
        if let Some(flag) = &opt_flag {
            cmd.arg(flag);
        }
        match cmd.output() {
            Err(e) if e.kind() == io::ErrorKind::NotFound && i + 1 < candidates.len() => {
                last_not_found = Some(e);
                continue;
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                if candidates.len() > 1 {
                    let _ = last_not_found;
                }
                return Err(DriverError::CcNotFound {
                    attempted: candidates
                        .iter()
                        .map(|c| c.to_string_lossy().to_string())
                        .collect(),
                });
            }
            Err(e) => {
                return Err(DriverError::Emit {
                    path: bin_path.to_path_buf(),
                    reason: format!("failed to run {}: {}", candidate.to_string_lossy(), e),
                });
            }
            Ok(out) if out.status.success() => return Ok(()),
            Ok(out) => {
                return Err(DriverError::Link {
                    status: out.status.to_string(),
                    stdout: String::from_utf8_lossy(&out.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&out.stderr).to_string(),
                });
            }
        }
    }
    Err(DriverError::CcNotFound {
        attempted: candidates
            .iter()
            .map(|c| c.to_string_lossy().to_string())
            .collect(),
    })
}

// ---------------------------------------------------------------------------
// Orchestration
// ---------------------------------------------------------------------------

/// Full compilation pipeline: validate -> read -> frontend -> lower -> emit
/// -> link. Errors are promoted to [`CompilerError`] so every stage renders
/// with the same structured shape (kind tag, span panel, help line). The two
/// frontend-only emits (`lex|parse|typed`, `--check`) never touch LLVM or
/// clang, so they work without the `llvm` feature.
pub fn compile(opts: &CompilationOptions) -> Result<CompileOutput, Box<CompilerError>> {
    compile_inner(opts).map_err(|e| match e {
        // Entry-file analysis errors get the full panel here: `compile_inner`
        // already holds the source text, so the offending line is extracted
        // and attached before the error leaves the driver. Parse errors carry
        // their own `filename`/`source_line`; module analysis errors keep
        // their module name as the filename but no caret (the module's source
        // is not retained), which `CompilerError` renders without one.
        DriverError::Analysis(ae) => {
            let mut ce = CompilerError::from(ae);
            if ce.filename.is_none()
                && let Some(sp) = ce.span
                && let Ok(source) = read_source(&opts.project_path, opts.source_override.as_deref())
                && let Some(line) = source_line_at(&source, sp.line)
            {
                ce.filename = Some(PathBuf::from(
                    opts.project_path.to_string_lossy().to_string(),
                ));
                ce.source_line = Some(line);
            }
            Box::new(ce)
        }
        other => Box::new(other.into()),
    })
}

fn compile_inner(opts: &CompilationOptions) -> Result<CompileOutput, DriverError> {
    let emit = opts.effective_emit();
    if !emit.needs_backend() {
        let frontend = run_frontend(opts)?;
        return Ok(CompileOutput::Frontend {
            emit,
            frontend: Box::new(frontend),
        });
    }

    // Backend requested: fail fast with a clear error when LLVM is off
    // instead of the old silent no-op binary.
    #[cfg(not(feature = "llvm"))]
    {
        // Still run the frontend first so syntax/type errors are reported
        // rather than masking them behind LlvmUnavailable.
        let _ = run_frontend(opts)?;
        return Err(DriverError::LlvmUnavailable(match emit {
            EmitKind::Llvm => "--emit=llvm",
            _ => "linking a binary",
        }));
    }

    #[cfg(feature = "llvm")]
    {
        let frontend = run_frontend(opts)?;
        let input = &opts.project_path;

        let mut typed = frontend.typed_program.clone();
        let imported = std::mem::take(&mut typed.imported_declarations);
        let local = std::mem::take(&mut typed.declarations);
        typed.declarations = dedupe_declarations(imported, local);
        typed.imported_declarations = Vec::new();

        let ir = lower_to_llvm_ir(typed, &frontend.filename)?;

        match emit {
            EmitKind::Llvm => {
                let binary_hint = opts
                    .output
                    .clone()
                    .unwrap_or_else(|| default_binary_path(input));
                let llvm_path = llvm_output_path(opts, emit, &binary_hint, input)
                    .unwrap_or_else(|| default_llvm_path(input));
                emit_llvm_ir(&ir, &llvm_path)?;
                Ok(CompileOutput::Llvm { path: llvm_path })
            }
            EmitKind::Bin => {
                let binary = binary_output_path(opts, input);
                match llvm_output_path(opts, emit, &binary, input) {
                    Some(llvm_path) => {
                        emit_llvm_ir(&ir, &llvm_path)?;
                        link_llvm_ir(
                            &llvm_path,
                            &binary,
                            opts.cc.as_deref(),
                            opts.opt_level.as_deref(),
                        )?;
                        Ok(CompileOutput::Binary {
                            binary,
                            llvm_ir: Some(llvm_path),
                        })
                    }
                    None => {
                        let tmp = tempfile::Builder::new()
                            .prefix("fibc-")
                            .suffix(".ll")
                            .tempfile()
                            .map_err(|e| DriverError::Emit {
                                path: std::env::temp_dir().join("fibc-.ll"),
                                reason: e.to_string(),
                            })?;
                        let tmp_path = tmp.path().to_path_buf();
                        emit_llvm_ir(&ir, &tmp_path)?;
                        let link_result = link_llvm_ir(
                            &tmp_path,
                            &binary,
                            opts.cc.as_deref(),
                            opts.opt_level.as_deref(),
                        );
                        // `tmp` deletes the file on drop; keep it alive until
                        // after linking, then drop explicitly.
                        drop(tmp);
                        link_result?;
                        Ok(CompileOutput::Binary {
                            binary,
                            llvm_ir: None,
                        })
                    }
                }
            }
            // Frontend emits returned early above; the match is exhaustive
            // for the type checker.
            _ => Ok(CompileOutput::Frontend {
                emit,
                frontend: Box::new(frontend),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_opts(source: &str) -> CompilationOptions {
        CompilationOptions {
            project_path: PathBuf::from("test.fib"),
            source_override: Some(source.to_string()),
            include_paths: Vec::new(),
            output: None,
            emit: EmitKind::Bin,
            check_only: false,
            emit_llvm: false,
            llvm_out: None,
            cc: None,
            opt_level: None,
        }
    }

    #[test]
    fn validate_rejects_non_fib_extension() {
        let err = validate_path(Path::new("main.c"), false).unwrap_err();
        assert!(matches!(err, DriverError::NotFibFile(_)));
    }

    #[test]
    fn validate_missing_file_errors() {
        let err = validate_path(Path::new("does-not-exist.fib"), false).unwrap_err();
        assert!(matches!(err, DriverError::NotAFile(_)));
    }

    #[test]
    fn validate_source_override_skips_existence_check() {
        assert!(validate_path(Path::new("virtual.fib"), true).is_ok());
    }

    #[test]
    fn read_source_prefers_override() {
        let src = read_source(
            Path::new("whatever.fib"),
            Some("fn main() @int { return 0; }"),
        )
        .expect("override");
        assert!(src.contains("main"));
    }

    #[test]
    fn frontend_runs_without_files_via_override() {
        let opts = test_opts("fn main() @int { return 0; }");
        let frontend = run_frontend(&opts).expect("frontend");
        assert_eq!(frontend.typed_program.declarations.len(), 1);
        assert!(!frontend.tokens.is_empty());
    }

    #[test]
    fn frontend_preserves_parse_error_structurally() {
        let opts = test_opts("fn broken( @int { return 0; }");
        let err = run_frontend(&opts).unwrap_err();
        match err {
            DriverError::Parse(pe) => {
                assert!(!pe.message.is_empty());
                assert_eq!(pe.filename.to_string_lossy(), "test.fib");
            }
            other => panic!("expected Parse, got {:?}", other),
        }
    }

    #[test]
    fn frontend_reports_analysis_error() {
        let opts = test_opts("fn main() @int { return undefined_var; }");
        let err = run_frontend(&opts).unwrap_err();
        assert!(matches!(err, DriverError::Analysis(_)));
    }

    #[test]
    fn compile_promotes_analysis_error_to_compiler_error_panel() {
        // `compile` is the CLI-facing boundary: an analysis error surfaces as
        // a `CompilerError` with the offending source line attached so the
        // caret panel renders, not as the internal `DriverError`.
        let src = "fn main() @int { return undefined_var; }";
        let mut opts = test_opts(src);
        opts.check_only = true;
        let err = compile(&opts).unwrap_err();
        assert_eq!(err.kind, crate::diagnostics::ErrorKind::Type);
        let text = err.to_string();
        assert!(text.starts_with("error[type]:"), "got: {}", text);
        assert!(text.contains("--> test.fib:1:"), "no location: {}", text);
        assert!(text.contains(src), "source line missing: {}", text);
        assert!(text.contains("^"), "caret missing: {}", text);
    }

    #[test]
    fn check_project_succeeds_on_valid_source() {
        let opts = test_opts("fn main() @int { return 0; }");
        let frontend = check_project(&opts).expect("check");
        assert_eq!(frontend.ast.declarations.len(), 1);
    }

    #[test]
    fn missing_import_fails_with_structured_error() {
        // Use a module name that cannot exist on disk so the failure is a
        // structured ImportNotFound (not a panic or generic string),
        // regardless of the test's working directory.
        let opts = test_opts("import nosuch::definitely_missing_xyz\nfn main() @int { return 0; }");
        let err = run_frontend(&opts).unwrap_err();
        assert!(
            matches!(err, DriverError::ImportNotFound { .. }),
            "got {:?}",
            err
        );
    }

    #[test]
    fn load_module_source_searches_roots_in_order() {
        let dir_a = tempfile::tempdir().expect("tmp a");
        let dir_b = tempfile::tempdir().expect("tmp b");
        fs::write(dir_a.path().join("mymod.fib"), "fn a() @int { return 1; }").unwrap();
        fs::write(dir_b.path().join("mymod.fib"), "fn b() @int { return 2; }").unwrap();
        let roots = vec![dir_a.path().to_path_buf(), dir_b.path().to_path_buf()];
        let (path, src) = load_module_source(&["mymod".to_string()], &roots).expect("load");
        assert_eq!(path, dir_a.path().join("mymod.fib"));
        assert!(src.contains("fn a"));
    }

    #[test]
    fn load_module_source_drop_first_segment_fallback() {
        // `import std::io` with `-I <std>` -> `<std>/io.fib`.
        let std_root = tempfile::tempdir().expect("std root");
        fs::write(std_root.path().join("io.fib"), "fn x() @int { return 1; }").unwrap();
        let roots = vec![std_root.path().to_path_buf()];
        let (path, _) = load_module_source(&["std".to_string(), "io".to_string()], &roots)
            .expect("load with fallback");
        assert_eq!(path, std_root.path().join("io.fib"));
    }

    #[test]
    fn resolve_imports_missing_module_reports_searched_roots() {
        let ast = {
            let src = "import nosuch::mod\nfn main() @int { return 0; }";
            let tokens = lex_source(src);
            parse_source(tokens, Path::new("t.fib"), src).expect("parse")
        };
        let err = resolve_imports(&ast, &[PathBuf::from("/nonexistent")]).unwrap_err();
        match err {
            DriverError::ImportNotFound { module, .. } => assert_eq!(module, "nosuch::mod"),
            other => panic!("expected ImportNotFound, got {:?}", other),
        }
    }

    #[test]
    fn resolve_imports_diamond_is_deduped() {
        // main -> {a, b}, a -> c, b -> c. c's decls must appear once after merge.
        let root = tempfile::tempdir().expect("proj");
        fs::write(root.path().join("c.fib"), "fn shared() @int { return 1; }").unwrap();
        fs::write(
            root.path().join("a.fib"),
            "import c\nfn fa() @int { return c::shared(); }",
        )
        .unwrap();
        fs::write(
            root.path().join("b.fib"),
            "import c\nfn fb() @int { return c::shared(); }",
        )
        .unwrap();
        let entry_ast = {
            let src = "import a\nimport b\nfn main() @int { return 0; }";
            let tokens = lex_source(src);
            parse_source(tokens, Path::new("main.fib"), src).expect("parse")
        };
        let roots = vec![root.path().to_path_buf()];
        let resolved = resolve_imports(&entry_ast, &roots).expect("resolve");
        assert!(resolved.contains_key(&vec!["a".to_string()]));
        assert!(resolved.contains_key(&vec!["b".to_string()]));
        assert!(resolved.contains_key(&vec!["c".to_string()]));

        let entry_typed = analyze_entry(entry_ast, &resolved).expect("analyze");
        let merged = dedupe_declarations(
            entry_typed.imported_declarations.clone(),
            entry_typed.declarations.clone(),
        );
        let shared_count = merged.iter().filter(|d| decl_key(d) == "fn:shared").count();
        assert_eq!(
            shared_count,
            1,
            "diamond import emitted twice: {:?}",
            merged.iter().map(decl_key).collect::<Vec<_>>()
        );
    }

    #[test]
    fn load_module_unifies_source_and_ast() {
        // (a): the single loader resolves the same file `load_module_source`
        // finds and parses it with file context for diagnostics.
        let dir = tempfile::tempdir().expect("tmp");
        fs::write(dir.path().join("mymod.fib"), "fn f() @int { return 1; }").unwrap();
        let roots = vec![dir.path().to_path_buf()];
        let key = vec!["mymod".to_string()];
        let (src_path, src) = load_module_source(&key, &roots).expect("source");
        let (path, source, ast) = load_module(&key, &roots).expect("load");
        assert_eq!(path, src_path);
        assert_eq!(source, src);
        assert_eq!(ast.declarations.len(), 1);
    }

    #[test]
    fn resolve_imports_alias_spellings_share_one_module() {
        // (b): the same file reached via two import spellings (full path +
        // drop-first-segment fallback) is loaded once and shared — both keys
        // resolve, with identical declarations, and the merge emits `shared`
        // exactly once.
        let root = tempfile::tempdir().expect("proj");
        let top = root.path().join("top");
        fs::create_dir(&top).expect("mkdir top");
        fs::write(top.join("m.fib"), "fn shared() @int { return 1; }").unwrap();
        let entry_ast = {
            let src = "import top::m\nimport m\nfn main() @int { return 0; }";
            let tokens = lex_source(src);
            parse_source(tokens, Path::new("main.fib"), src).expect("parse")
        };
        let roots = vec![root.path().to_path_buf(), top.clone()];
        let resolved = resolve_imports(&entry_ast, &roots).expect("resolve");
        let full_key = vec!["top".to_string(), "m".to_string()];
        let short_key = vec!["m".to_string()];
        assert!(resolved.contains_key(&full_key), "full spelling resolves");
        assert!(
            resolved.contains_key(&short_key),
            "fallback spelling resolves"
        );
        assert_eq!(
            resolved[&full_key]
                .declarations
                .iter()
                .map(decl_key)
                .collect::<Vec<_>>(),
            resolved[&short_key]
                .declarations
                .iter()
                .map(decl_key)
                .collect::<Vec<_>>(),
            "both spellings share the same module content"
        );

        let entry_typed = analyze_entry(entry_ast, &resolved).expect("analyze");
        let merged = dedupe_declarations(
            entry_typed.imported_declarations.clone(),
            entry_typed.declarations.clone(),
        );
        assert_eq!(
            merged.iter().filter(|d| decl_key(d) == "fn:shared").count(),
            1,
            "aliased module emitted twice: {:?}",
            merged.iter().map(decl_key).collect::<Vec<_>>()
        );
    }

    #[test]
    #[cfg(feature = "llvm")]
    fn diamond_import_end_to_end_single_definition() {
        // (c): full pipeline proof that driver-side dedupe covers the removed
        // lowering-side duplicate guards. The entry lives on disk (no
        // `source_override`, so the unified entry loader is exercised) with a
        // diamond (main -> {a, b}, a -> c, b -> c).
        // - Pre-dedupe the frontend really does see `shared` twice (the test
        //   is vacuous without the diamond hazard), post-dedupe exactly once
        //   with no duplicate keys at all (covers the llvm_lower failure
        //   mode: same-name second body reusing the LLVM function).
        // - The emitted LLVM IR defines `@shared` exactly once with no
        //   renamed duplicate (`@shared.1`, the ir_lower failure mode).
        let root = tempfile::tempdir().expect("proj");
        fs::write(root.path().join("c.fib"), "fn shared() @int { return 1; }").unwrap();
        fs::write(
            root.path().join("a.fib"),
            "import c\nfn fa() @int { return c::shared(); }",
        )
        .unwrap();
        fs::write(
            root.path().join("b.fib"),
            "import c\nfn fb() @int { return c::shared(); }",
        )
        .unwrap();
        let main = root.path().join("main.fib");
        fs::write(
            &main,
            "import a\nimport b\nfn main() @int { return a::fa() + b::fb(); }",
        )
        .unwrap();

        let mut entry_opts = CompilationOptions::new(main.clone());
        entry_opts.emit = EmitKind::Typed;
        let frontend = run_frontend(&entry_opts).expect("frontend");
        let pre = frontend
            .typed_program
            .imported_declarations
            .iter()
            .filter(|d| decl_key(d) == "fn:shared")
            .count();
        assert_eq!(pre, 2, "fixture must duplicate `shared` pre-dedupe");
        let merged = dedupe_declarations(
            frontend.typed_program.imported_declarations.clone(),
            frontend.typed_program.declarations.clone(),
        );
        let mut seen = HashSet::new();
        for d in &merged {
            assert!(
                seen.insert(decl_key(d)),
                "duplicate surviving dedupe: {} in {:?}",
                decl_key(d),
                merged.iter().map(decl_key).collect::<Vec<_>>()
            );
        }
        assert_eq!(
            merged.iter().filter(|d| decl_key(d) == "fn:shared").count(),
            1
        );

        let ll_path = root.path().join("out.ll");
        let mut opts = CompilationOptions::new(main);
        opts.emit = EmitKind::Llvm;
        opts.output = Some(ll_path.clone());
        match compile(&opts).expect("compile diamond") {
            CompileOutput::Llvm { path } => assert_eq!(path, ll_path),
            other => panic!("expected Llvm output, got {:?}", other),
        }
        let ir = fs::read_to_string(&ll_path).expect("read ll");
        let defines: Vec<&str> = ir
            .lines()
            .filter(|l| l.contains("define") && l.contains("@shared"))
            .collect();
        assert_eq!(
            defines.len(),
            1,
            "expected exactly one @shared definition, got {:?}\n{}",
            defines,
            ir
        );
    }

    #[test]
    fn resolve_imports_circular_reports_full_stack() {
        let root = tempfile::tempdir().expect("proj");
        fs::write(
            root.path().join("a.fib"),
            "import b\nfn fa() @int { return 1; }",
        )
        .unwrap();
        fs::write(
            root.path().join("b.fib"),
            "import a\nfn fb() @int { return 1; }",
        )
        .unwrap();
        let entry_ast = {
            let src = "import a\nfn main() @int { return 0; }";
            let tokens = lex_source(src);
            parse_source(tokens, Path::new("main.fib"), src).expect("parse")
        };
        let err = resolve_imports(&entry_ast, &[root.path().to_path_buf()]).unwrap_err();
        match err {
            DriverError::CircularImport { stack } => {
                assert!(stack.len() >= 2);
                assert!(stack.iter().any(|s| s == "a"));
            }
            other => panic!("expected CircularImport, got {:?}", other),
        }
    }

    #[test]
    fn dedupe_local_shadows_imported() {
        let src = "fn main() @int { return 0; }";
        let mk = |name: &str| {
            let tokens = lex_source(&format!("fn {}() @int {{ return 1; }}", name));
            let ast = parse_source(tokens, Path::new("m.fib"), src).expect("parse");
            let typed = analyze_entry(ast, &HashMap::new()).expect("analyze");
            typed.declarations.into_iter().next().unwrap()
        };
        let imported_fn = mk("foo");
        let local_fn = mk("foo");
        let merged = dedupe_declarations(vec![imported_fn], vec![local_fn]);
        assert_eq!(merged.len(), 1);
    }

    #[test]
    fn emit_kind_from_str() {
        use std::str::FromStr;
        assert_eq!(EmitKind::from_str("lex").unwrap(), EmitKind::Lex);
        assert_eq!(EmitKind::from_str("LLVM").unwrap(), EmitKind::Llvm);
        assert!(EmitKind::from_str("nope").is_err());
        assert!(EmitKind::Bin.needs_backend());
        assert!(!EmitKind::Typed.needs_backend());
    }

    #[test]
    fn default_paths_derive_from_stem() {
        assert_eq!(
            default_binary_path(Path::new("samples/hello.fib")),
            PathBuf::from("out/hello")
        );
        assert_eq!(
            default_llvm_path(Path::new("samples/hello.fib")),
            PathBuf::from("out/hello.ll")
        );
    }

    #[test]
    fn llvm_path_selection() {
        let input = Path::new("src.fib");
        let bin = PathBuf::from("build/prog");
        let base = CompilationOptions::new(input.to_path_buf());
        // --emit=llvm defaults to out/<stem>.ll
        assert_eq!(
            llvm_output_path(&base, EmitKind::Llvm, &bin, input).unwrap(),
            default_llvm_path(input)
        );
        // --emit=bin without keep flags -> temp file (None)
        assert!(llvm_output_path(&base, EmitKind::Bin, &bin, input).is_none());
        // --emit-llvm keeps alongside the binary
        let mut keep = base.clone();
        keep.emit_llvm = true;
        assert_eq!(
            llvm_output_path(&keep, EmitKind::Bin, &bin, input).unwrap(),
            PathBuf::from("build/prog.ll")
        );
        // explicit --llvm-out wins
        let mut explicit = base.clone();
        explicit.llvm_out = Some(PathBuf::from("x/y.ll"));
        assert_eq!(
            llvm_output_path(&explicit, EmitKind::Bin, &bin, input).unwrap(),
            PathBuf::from("x/y.ll")
        );
    }

    #[test]
    fn cc_candidates_respect_explicit_and_env() {
        // Explicit --cc wins over everything.
        let cands = cc_candidates(Some(Path::new("/opt/clang")));
        assert_eq!(cands, vec![PathBuf::from("/opt/clang")]);
        // Default falls back clang-17 -> clang.
        unsafe { std::env::remove_var("CC") };
        let cands = cc_candidates(None);
        assert_eq!(
            cands,
            vec![PathBuf::from("clang-17"), PathBuf::from("clang")]
        );
    }

    #[test]
    fn invalid_opt_level_rejected() {
        let dir = tempfile::tempdir().expect("tmp");
        let ll = dir.path().join("t.ll");
        let bin = dir.path().join("t");
        fs::write(&ll, "ir").unwrap();
        let err = link_llvm_ir(&ll, &bin, None, Some("fast")).unwrap_err();
        assert!(matches!(err, DriverError::InvalidOptLevel(_)));
    }

    #[test]
    fn compile_frontend_emits_need_no_backend() {
        for emit in [EmitKind::Lex, EmitKind::Parse, EmitKind::Typed] {
            let mut opts = test_opts("fn main() @int { return 0; }");
            opts.emit = emit;
            let out = compile(&opts).expect("frontend emit");
            match out {
                CompileOutput::Frontend { emit: e, .. } => assert_eq!(e, emit),
                other => panic!("expected Frontend, got {:?}", other),
            }
        }
    }

    #[test]
    fn compile_check_only_overrides_emit() {
        let mut opts = test_opts("fn main() @int { return 0; }");
        opts.emit = EmitKind::Bin;
        opts.check_only = true;
        let out = compile(&opts).expect("check");
        match out {
            CompileOutput::Frontend { emit, .. } => assert_eq!(emit, EmitKind::Typed),
            other => panic!("expected Frontend, got {:?}", other),
        }
    }
}
