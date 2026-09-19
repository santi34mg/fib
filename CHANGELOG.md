# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-09-18

### Added
- End-to-end regression net (`tests/e2e.rs`): compiles every `samples/*.fib`
  through the library API and executes the result, with exact-stdout asserts
  for deterministic samples (`fib_bench` is compile-only).
- IR middle-end (`src/ir/`): `SymbolId`-based 3-address IR with
  `lower_typed_program`, pretty-printing, and contract tests.
- Staged driver/CLI (`src/driver.rs`, `src/cli.rs`): `--emit=lex|parse|typed|llvm|bin`,
  `--check` frontend-only mode, `--emit-llvm`/`--llvm-out`, `--cc`, `-O`,
  `-I` include paths, module dedup.
- Compiler audit series (`docs/audit/01`-`06`): architecture, reliability,
  driver/CLI, frontend, testing, project health.
- CI hardening: `fmt` + `frontend (--no-default-features)` + LLVM
  `build_and_test` + advisory `cargo audit` jobs; docs-only changes skip CI.
- Driver cutover to the IR middle-end: `lower_to_llvm_ir` tries
  `ir::lower_typed_program` then the `ir_lower` consumer, and falls back to
  the direct path for anything outside the core subset (never silently
  miscompiles).
- Module resolution: canonicalized-path interning (`canonicalize_module_path`
  + `path_intern`) so alias spellings of one file share a `TypedModule`;
  diamond imports proven deduped by a full-pipeline test; lowering-side
  duplicate guards removed.
- Typed backend errors: `LowerError` (`MissingBlock`/`UnknownLayout`/
  `Unsupported`/`Llvm`) replaces `Box<dyn Error>` across all lowering paths,
  with `UnknownLayout` folded into `map_type_to_llvm`.
- Execution tests for unsigned ops (`samples/unsigned_ops.fib`,
  `e2e_unsigned_ops`) with values above `iN::MAX` (udiv/urem/ucmp/lshr).
- Toolchain pin `rust-toolchain.toml` (hashed into the CI cache key) plus
  advisory, non-blocking `cargo deny` and `tarpaulin` coverage jobs.
- `FunctionLowering` owning struct (`lowering/context.rs`): holds
  `ctx`/`function`/`vars`/`scope`/`deferred_stack`/`loop_ctx`; methods
  `insert_block` (fn-name-aware), `parent_function`, `loop_ctx`,
  `enter_loop`/`exit_loop`, `emit_deferred_frame`, `emit_frames_from`.
  `codegen_expr`/`codegen_stmt` and friends are methods; the module-level
  `const` branch builds its own `FunctionLowering`.
- Control-flow dedup helpers: `emit_sequence_with_fallthrough` (shared by
  `if` branches, `switch` arms + wildcard default, `for` bodies),
  `emit_loop` (owns the whole `for` arm; `post` keeps the enclosing
  deferred frame), and `emit_short_circuit` (`&&`/`||` rhs/merge + phi).
- Structured diagnostics (`src/diagnostics.rs`): one `CompilerError`
  (`ErrorKind` tag, `Span{line,col}`, optional hint, `filename`/`source_line`
  render context) with a rustc-style caret panel, hand-rolled (no
  miette/thiserror). Every stage promotes into it — `Parser`/`DriverError`
  via `From`, `AnalysisError` keeping its kind and span — and the CLI is a
  single print path via `compile()` returning `Result<_, Box<CompilerError>>`.
- Frontend spans: `Statement { kind, span }`; `Expression` and
  `TypeExpression` are now `{ kind, span }` wrappers over `ExpressionKind` /
  `TypeExpressionKind`, recorded from the node's first token. The analyzer
  inherits them via `with_span_fallback` on `expr_to_typed`, `map_type`, and
  `stmt_to_typed`, so errors point at their own node without per-site churn;
  dozens of error sites carry `= help:` hints (immutable assignment,
  uninferred decl, unknown function → `extern`).

### Changed
- Split `ir`, `analyze`, and lowering into modules mirroring the parser layout.
- Frontend: generic binary-expression parser; lexer split (`frontend/lexer/`);
  `BinOp` decoupled from lowering.
- Reliability: eliminated panics in lowering paths; closed assignment
  soundness holes (negative `get_typed_err` tests).
- Analyze: loop-depth-aware `break`/`continue` checks (nested loops fine,
  top-level rejected), enum-switch exhaustiveness (wildcard via `when else`),
  and per-block return-type validation sharing the assignment
  `check_assignable` rule; `get_ast` returns `Result` with ~16 parse-error
  negatives; lexer char/hex/string-escape error branches.
- Unsigned handling: `zext` widening, `udiv`/`urem`/`ucmp` opcode selection
  keyed off operand type; extracted `map_type_to_llvm` into `lowering/types.rs`.

### Removed
- Dropped the self-hosted compiler effort (`compiler.fib`, 518 lines) to
  focus on the Rust/LLVM compiler.

[Unreleased]: https://github.com/santi34mg/fib/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/santi34mg/fib/releases/tag/v0.1.0
