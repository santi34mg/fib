# Driver, CLI, and Module Resolution

## Current state (verified)

- `src/driver.rs:83-183` `compile()` is a ~100-line monolith: file-extension check -> `fs::read_to_string` -> `Lexer::collect` -> `Parser::parse` (aborts on first `ParseError` via `eprintln` + `Err("Parser error.")`) -> import scan -> `resolve_module` -> `analyze` -> drain/chain `imported_declarations` -> `lowering::lower` -> `fs::create_dir_all("out")` -> write `out/{stem}.ll` -> invoke `clang-17` with fallback to `clang`.
- `src/driver.rs:187-265` `resolve_module()` re-lexes/parses each import manually (`FIXME:234`), recurses on nested imports, detects cycles via `resolving: Vec<Vec<String>>`, tries each `search_roots` in order with a fallback that drops the first path segment (so `import std::io` with `-I std` resolves to `<std>/io.fib`).
- `src/cli.rs:9-15` has only `FILE` + `-I/--include-path: Vec<PathBuf>`. `CompilationOptions::new(args)` (`driver.rs:39-45`) just moves them. `src/lib.rs:17-19` gates `compile_project` behind `#[cfg(feature="llvm")]`; `src/main.rs:1-6` is a silent no-op without it.

## Points of interest

### 1. Split `compile()` into stages
Suggested shape: `validate_path -> read_source -> run_frontend (tokens+ast+typed) -> resolve_imports -> lower -> emit -> link`.

Trade-offs:
- **Pro:** each stage becomes unit-testable (today you cannot test import resolution without invoking clang). Enables `--emit=lex|parse|typed|llvm` for debugging.
- **Con:** more types (`FrontendResponse` already exists at `driver.rs:21-27` but is unused by `compile()` — wire it up or delete it). Avoid over-engineering a pass manager; free functions returning `Result` are enough for now.

### 2. Hard-coded `out/` and clang invocation
`driver.rs:148-156` always writes to CWD-relative `out/`, derives names from `file_stem`. `158-170` shells to `clang-17` then `clang`, treats non-zero exit as string error.

Opportunities: `--output/-o`, `--emit-llvm` (keep `.ll`), `--cc` (clang path), `--opt-level`. Respect `CC` env var. Use `tempfile` for intermediate `.ll` unless `--emit-llvm` is set.

Trade-offs:
- **Flexibility vs. simplicity.** `cargo run -- samples/hello_world.fib -I=std` is delightfully simple today. Adding flags risks flag bloat, but hard-coded `out/` breaks concurrent builds, library use (`compile_project` should not `create_dir_all` as a side effect), and Windows paths.
- **Linking via Command vs. inkwell emit.** Shelling to clang is pragmatic (handles runtime libs, LTO flags). Direct object emission via inkwell removes the clang dependency but forces you to reimplement linking. Keep shell-out, just make it configurable and report `stdout/stderr` distinctly.

### 3. Module resolution duplication
`resolve_module` duplicates top-level lex/parse/analyze and merges declarations by `drain(..).chain(..)` (`driver.rs:138-143`). Risks: diamond imports emit duplicate bodies (mitigated only by `llvm_lower.rs:102-103` skip-if-has-body hack), no caching of parse errors with file context, confusing two-way search (full path vs. drop-first-segment).

Opportunities:
- Canonicalize `Vec<String>` keys + file-path interning; cache `Ast`/`TypedModule` by resolved path.
- Emit duplicate-declaration handling in the driver (dedupe by symbol), not in LLVM lowering.
- Replace `resolving: Vec` linear scan with `HashSet` for cycle detection + full import stack in error message.

Trade-offs:
- **Correctness vs. velocity.** A real module system (visibility, separate compilation, incremental cache) is a multi-week project. Short term, just extract `load_module(path, search_roots)` returning `Result<(PathBuf,String,Ast)>` and reuse it for both entry file and imports — removes the `FIXME` without redesigning modules.

### 4. Feature-gating and library API
`lib.rs:17` + `main.rs:5` mean `cargo build --no-default-features` yields a binary that parses args and exits 0 doing nothing, and no library API at all.

Options:
- A) Make frontend (`lexer/parser/analyze`) always available; gate only `lowering + link`. Then `fibc --check` works without LLVM — great for CI/docs tests.
- B) Emit `compile_error!` / runtime error when `llvm` is off.
- Trade-off: (A) is strictly better for testability but requires untangling `driver.rs` imports gated on `#[cfg(feature="llvm")]`. Small refactor, high value.

### 5. Parser error short-circuit
`driver.rs:100-106` prints one `ParseError` and returns `Err("Parser error.")`, discarding structured info. `FrontendResponse{parse_errors, analysis_errors}` suggests batched diagnostics were intended.

Trade-off: batching errors is better UX but harder (error recovery in hand-written recursive descent). At minimum, preserve the original `ParseError` via `#[from]` instead of stringifying to `"Parser error."`.
