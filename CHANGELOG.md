# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- An unfinished `BuilderObject` build-file pipeline with build stages; compilation and interpretation of build files, object emission, and linking are not yet implemented end-to-end.
- Split tokenizer and new token shapes for the upcoming parser migration; the current compiler still uses the existing lexer.
- Opt-in statement-only parser and AST modules for the new syntax, including bit-width builtin type names, per-binding types and `const` in multi-variable declarations, and qualified-access module aliases instead of import declarations.
- Opt-in lexical name resolution for the new AST, with scoped declaration IDs and diagnostics for unknown or duplicate names. Module-qualified imports and type checking remain future work.
- Preserved the unfinished symbol-table and IR rewrite sources from `refactor/modules-and-generic-functions`; the IR interpreter is now exported but remains unimplemented.

### Changed
- Builtin numeric type variants and displayed names use bit widths (`Int32`, `int32`); bare `int` and `uint` now denote 32-bit types. The production lexer still accepts legacy numbered `@int4`-style source spellings until the parser migration.
- The opt-in parser uses postfix `&`/`*`, direct indexing and slicing (`arr[i]`, `arr[a..b]`), and `.len` instead of the established compiler's dotted access forms.
- Replaced the legacy driver, CLI, and library entry points with the unfinished build-file pipeline from `refactor/modules-and-generic-functions`. `build`, `run`, and `test` do not work yet, and this branch does not currently compile: the driver still expects an analyzer and IR output that are not wired up.
- Extracted the existing flat IR instruction definitions into their own module without changing lowering behavior.

### Fixed
- The tokenizer now rejects an unescaped quote in a character literal, so `'''` (and the bare forms `''` and `'''''`) no longer parse as a valid character literal; a literal quote must be written as an escape (`'\''`).

## [0.1.1] - 2026-10-14

### Added
- Slice types (`T[]`), explicit array views and sub-slicing, inclusive and
  exclusive ranges, indexed reads and writes, and slice arguments and returns.
- `@usize` and the `.@len` property for arrays and slices.
- Compile-time diagnostics for constant out-of-bounds array indices and invalid
  slice ranges, plus default runtime bounds checks that print an error and abort.
- `--release` to disable runtime bounds checks while retaining compile-time
  checks. Raw pointer indexing remains unchecked.
- Repeatable `-l` / `--link-lib` options and library API support for native
  library linkage.
- Expanded experimental standard library modules and libc bindings.
- A JavaScript language server providing semantic highlighting and a VS Code
  extension to launch it.
- Slice and bounds-check regression tests and a pre-commit validation gate.

### Changed
- **Breaking:** statements now require trailing semicolons, including block
  statements such as `if`, `while`, and `switch`.
- **Breaking:** replaced C-style `for` loops with `while (condition)` loops;
  initialization and updates must be written as separate statements.
- Updated maintained samples to use mandatory semicolons and `while` loops.

### Fixed
- Added the missing `cargo-deny` license policy and package MIT license metadata,
  and included dependency policy checks in the pre-commit hook.
- Prefer Clang 21 when linking LLVM 21 IR, avoiding failures when an incompatible
  `clang-17` is also installed.

### Known limitations
- Fourteen of the 25 standard library modules fail frontend import checks due
  to remaining uses of removed `for` syntax, directly or through dependencies.
  Some documentation examples also retain the old syntax.
- The standard library remains experimental: some networking operations are
  placeholders, atomic APIs use ordinary loads/stores, and pthread storage
  sizes are platform-specific assumptions.
- The language server protocol test hangs; highlighting tests pass.
- Semantic validation, IR verification, target-aware layout, ownership, and
  complete flat-IR feature support remain ongoing work.

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

[Unreleased]: https://github.com/santi34mg/fib/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/santi34mg/fib/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/santi34mg/fib/releases/tag/v0.1.0
