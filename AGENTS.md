# AGENTS.md

Fib (`fibc`) is a Rust compiler under reconstruction. Start with the
[current implementation guide](docs/agents/README.md), not historical compiler claims.

## Current checkout

- Working: `src/frontend/tokenizer.rs` + `tokenizer/`, `parser.rs` + `parser/`,
  AST construction, lexical scope utilities, diagnostics and raw IR data utilities.
- `analyzer.rs` + `analyzer/` are explicit-error sketches. Analysis annotates the
  original AST in the intended architecture; no lexer/analyze/typed_ast modules exist.
- `src/driver.rs` exposes `run_pipeline`, `run_frontend`, `parse_files`,
  `lower_to_ir`, all unimplemented. No `compile_project`/`check_project` APIs.
- `BuildPlan` explicitly names files; single-file loading, build.fib interpretation,
  builder orchestration and IR interpreter are unfinished (`todo!()` or errors).
- `src/ir/` is an owned SSA representation, not the former flat importer.
  Builder/verifier always error. Backend is an IR-only sketch, available without
  LLVM; no emission or direct AST fallback. `backend/lowering/next/` is uncompiled
  legacy code referencing removed APIs, not a supported route.
- CLI accepts `build [--single-file PATH]`, `run`, `test`; execution is `todo!()`.
  Help works. Old positional-file, include-root, check/emit/link/release flags do not.

## Toolchain and commands

`rust-toolchain.toml` and CI pin 1.98.0 (MSRV policy); bump deliberately together.
Rust 2024 package; default `llvm` enables Inkwell 0.8 / LLVM 21 dependency builds,
not working LLVM emission. No-default-features work needs no LLVM installation.

Verified on system Cargo/rustc 1.98.0 (offline cache present):

```sh
cargo check --offline --no-default-features
cargo test --offline --no-default-features
cargo run --offline --no-default-features -- --help
```

Check/help succeeded and tests passed 173 library tests; no integration/e2e suite
exists. Offline is optional outside restricted environments. Focus tests with
`cargo test --no-default-features frontend::parser` or `frontend::tokenizer`.
See [validation status](docs/agents/tests/README.md) for formatting and limitations.

CI config runs fmt; no-default check/test; LLVM 21 fmt/check/clippy
(`--all-targets -- -D warnings`)/build/test/all-features. These are configured gates,
not verified green status. Audit/deny/coverage are advisory. The optional local
pre-commit hook additionally makes deny blocking. CI skips docs-only paths.

## Syntax and source guidance

Use bare bit-width types (`int32`, `uint64`, aliases `int`/`uint` = 32-bit),
`x := 1;`, `const x := 1;`, `x: int32;`, `fn f(x: int32) int32 { return x; }`,
`while (condition) { ... };`, `a[i]`, `a[a..b]`, `.len`, postfix pointer operations.
Builtin functions retain `@`; legacy `@` types, `var`, `for`, dot-index and imports
are not the current working grammar. Parsing is not semantic/runtime support.
See [parser details](docs/agents/src/frontend/parser.md). Samples use current
syntax and have automatic parser coverage; std sources may still be stale.

`FileId`/`FileName`/paths live in `build_plan.rs`; syntax retains point spans and
pending analysis slots. There is no central source-map store. Prefer
`Parser::with_source` for in-memory tests; preserve source context and identities
when implementing missing passes. See [diagnostics](docs/agents/src/diagnostics.md).

## Tests and contribution discipline

- Add tokenizer/parser success and malformed-input tests for syntax changes.
- Analyzer/driver/IR/backend tests mostly protect data or explicit stub failure,
  not successful semantic analysis, verification or program execution.
- Never turn unfinished operations into successful empty results. Implement the
  root invariant, then add real positive/negative coverage. Assert diagnostic
  category/substrings/locations instead of brittle full rendered strings.
- Future lowering branches should ship sample reproducers and executable tests
  once a harness exists; current samples are automatically parsed, not compiled
    or run. Use `cargo test --no-default-features samples_parse_with_current_frontend`.
- Keep changes focused; preserve user work. Do not chase advisory coverage totals.
- Preserve trunk-based/main and Conventional Commit PR guidance; update
  `CHANGELOG.md` under `[Unreleased]` for behavior changes when in scope.
- Update mirrored agent docs with implementation changes. Clearly distinguish
  working code, representation contracts, sketches, and future policy.
- Keep verified tokenizer/parser behavior in `docs/parsing/`, not as the intended
  language specification. Parser limitations during the refactor are not language
  decisions. Track analysis gaps in GitHub issues, not local report copies.
  Keep only GitHub links in `docs/agents/issues/README.md`.
