# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Language server

- Replace the standalone JavaScript highlighter with a native `fib-lsp` Rust binary using the compiler frontend, with semantic tokens, collecting analyzer diagnostics/warnings, unsaved source overlays and `build.fib` project planning. Update editor launchers to use the native executable; install with `cargo install --path . --no-default-features --bin fib-lsp`.


### Removed

- Removed unused backend phase-planning APIs, legacy builtin tokens and byte-width lookup, the unproduced qualified-access AST variant, and the experimental standard-library tree. Active primitive types and LLVM emission remain supported. Renamed frontend implementation guides to match current modules and updated references.

### Added

- Specification execution support: ordinary protected prelude type identifiers;
  direct same-family literal cast context; numeric/bool conversions; shallow
  readonly views; one-layer checked pointer field access with warnings; exhaustive
  enum switches; multi-file calls and concrete generic specialization; fixed-array
  value storage/copies/calls/lengths and multi-binding locals. Explicit package
  metadata and scoped visibility support file, subtree and package access with
  identity-preserving re-exports. Discovery/package-path integration and runtime
  indexing/slices remain deferred.

- Executable enums: payloadless and payload constructors, scoped payload copies, switches, storage/pointers and internal helper calls now lower through verified SSA and LLVM. Target-derived tag-plus-union layouts, enum constants, active-variant extraction checks, switch PHIs/backedges and a runnable `enum_runtime.fib` sample have structural and executable coverage. See `docs/enum-semantics.md`.
- Native structs and typed raw-pointer storage: construction/copies, nested fields, partial initialization, address-taking, alias mutation and direct internal struct/pointer calls. Ordinary dereference traps on null; the compiler-provided `memory::unchecked_deref(pointer)` returns a typed place without that guard. Memory IR verification, LLVM aggregates/stack operations, and semantic/structural/executable regressions accompany a runnable struct sample.
- A runnable `example-project` demonstrates current scalar types, casts, bit operations, recursion, conditionals and loops with an interpreted build file, self-checking exit statuses and frontend/IR plus CLI executable coverage.
- Scalar `while` loops, `break` and `continue` now lower through verified SSA and LLVM, carrying initialized locals across backedges and merging loop exits without storage. Nested loops, early returns and short-circuit conditions have IR and bounded executable coverage. Build-file interpretation explicitly rejects unsupported CFG before applying operations.
- Complete fixed-width scalar lowering through verified SSA and LLVM: signed/unsigned 8–128-bit integers, float16/32/64/128 and bool support helpers, calls, locals, joins, arithmetic, comparisons, explicit numeric casts and short-circuit logic. Integer arithmetic wraps; invalid division, shifts and float-to-integer conversions trap. Exact integer/IEEE literal and constant evaluation replaces u64/f64 payload limits. Internal helpers use the limited scalar Fib fastcc ABI; numeric type changes require explicit casts. See `docs/scalar-semantics.md`.
- Interpreted build files now finalize their host-owned state into a validated, deterministic `BuildPlan`: source and output paths are project-root-relative, sources receive ordered compiler IDs and stem-derived logical names, selected entries must exist, and duplicate names/paths, missing files, root escapes and source-overwriting outputs are rejected. `build --build-file` feeds the resulting plan into the normal binary pipeline.
- Initial interpreted-build API foundation: synthetic-module semantic builtins (`build_api::add_source`, `build_api::set_entry`, `build_api::set_output`) have stable identities and typed `string -> void` signatures; the shared lowering emits verified Fib declarations/calls with interpreter-owned byte constants, and the interpreter dispatches them by identity into `PlanBuilder` state. The intentionally narrow source subset currently requires one `build() int32`, direct builtin calls with string literals, and an integer return.
- Independent pointee/element mutability with `const *T`, `const T[]` and `const T[N]`, including nested qualifiers, qualified address/view propagation and readonly-preserving casts.
- Explicit `comp fn` declarations and function types gate bounded type computation independently of known function identity. Concrete generic instances own rechecked bodies/binding environments; deferred type computations retain argument dependencies. Dependent arithmetic/member/index operations are checked on specialization.
- Directional `never` bottom coercion and divergence propagation, with normal completion forbidden for `never` functions. Independent annotation validation checks return contexts, literal kinds and cast legality.
- Multi-block LLVM emission with PHIs, backedges, parallel-edge splitting and signed/unsigned comparisons. Single-file lowering now supports int32 helpers/direct calls, initialized int32/bool locals, assignments, nested if/else and early returns using SSA merges. Per-function ABI classification and a shared LLVM boundary adapter keep physical argument placement separate from logical IR; the C 32-bit scalar ABI remains supported; internal helpers now use the limited scalar Fib ABI described above. TDD coverage includes executable conditionals, hand-built CFGs, mixed test ABI dispatch and located subset diagnostics. Analyzed projects retain source paths for lowering errors.
- First executable single-file compiler milestone: `fibc build --single-file PATH` analyzes a lone `main() int32` returning an integer literal, constructs and verifies owned SSA, emits verified LLVM 21 and a PIC object, and links with host `cc`. Transactional IR construction and scalar ownership/signature/dominance verification reject invalid or unsupported input. Acceptance tests compile/link/run exit codes 0, 42 and 127; general lowering, representations and ABI/CFG planning remain unfinished.
- Semantic type resolution on the original AST: contextual inference, canonical composite types, nominal struct/enum identities, recursive-type size checks, operators/casts/places, ordinary calls/returns, aggregates, slices, pattern payloads and owner-based members. `Analyzer::resolve_types` returns semantic tables; `validate_type_annotations` rejects incomplete or inconsistent annotations independently of full analysis.
- Explicit metatype signature specialization and caching, bounded type-return evaluation with known type/bool arguments, definite initialization, return-path validation, enum-switch flow and reverse-order scope-exit defer checks. `Analyzer::analyze` now returns a validated project. All six former pending acceptance cases and added positive/negative completion regressions pass in the unfiltered no-default-features suite.
- Working name resolution through the existing staged analyzer API: annotated declarations, parameters/locals, qualified build-plan paths, visibility, canonical compile-time aliases, cycle diagnostics, and name-slot completion. Six independently runnable specification phases cover identities, traversal, declaration order, source preservation, and error cleanup. Initialization and structured control-flow validation now run separately from name/type resolution.
- A local Zed extension in `lsp/editors/zed` registers `.fib` files without a Tree-sitter grammar and launches the existing JS highlighting server via `fib-lsp` on PATH or a configurable binary override; setup documents semantic-token opt-in and Rust/Wasm requirements.
- Backend-independent SSA `Ir` contracts with explicit function ownership, block parameters and edge arguments, typed values, memory operations, and fallible builder/verifier sketches.
- In-place AST analysis annotations, semantic type and resolver-context contracts, plus an LLVM adapter phase plan covering verification, target layout, ABI classification, and SSA edge translation. Lowering, IR verification, and code generation remain unfinished.
- Parser and architecture-contract regression tests; deleted integration tests remain deleted.
- An automatic parser regression test discovers every top-level `samples/*.fib` file; it does not perform semantic analysis or execution.
- Documented file namespaces, `comp` aliases, visibility, lexical lookup, and planned plain function-pointer semantics; ordinary function-pointer calls now have type checks; complete analysis now adds initialization/flow checks.
- An unfinished `BuilderObject` build-file pipeline with build stages; compilation and interpretation of build files, object emission, and linking are not yet implemented end-to-end.
- Split tokenizer and token shapes used by the current parser and its tests; there is no separate production lexer module.
- Current parser and AST support bare bit-width builtin type names, per-binding types and `const` in multi-variable declarations, and qualified identifier syntax. Import declarations are not implemented; name resolution uses explicitly named build-plan files and aliases.
- Lexical scope utilities and declaration-identity contracts; semantic completion now includes initialization/flow validation.
- Preserved the unfinished symbol-table and IR rewrite sources from `refactor/modules-and-generic-functions`; the IR interpreter is now exported but remains unimplemented.

### Changed
- Expanded the runnable example project to two source files with struct/pointer,
  enum/switch, generic, fixed-array and multi-binding demonstrations, checked by
  the frontend/IR and executable acceptance tests.
- Documented the language specification: package-rooted file namespaces/source discovery and scoped visibility; protected implicit-prelude type identifiers; retained suffix arrays; contextual direct-literal casts and explicit numeric/bool conversion; independent array mutability, readonly views, warned one-layer pointer field access and exhaustive enum switches. Compiler implementation status is documented separately.
- Refreshed roadmap and implementation documentation for executable enums, explicit interpreted build files and current test coverage. Consolidated generic and array rules into the language specification and removed superseded proposal pages.
- Moved analyzed-source-to-IR lowering from the driver into one `ir::lowering::lower` entry point. Build files and normal sources now use the same analyzer and lowering; `build_api` is configured as a synthetic module, while the driver only validates the entry contract and selects LLVM or interpretation.
- **Breaking:** removed variadic syntax, AST/semantic/IR signature flags and historical variadic libc declarations (fixed-signature C shims are required; old std callers remain unmigrated). Type-return evaluation now requires explicit `comp fn`; ordinary runtime functions are not evaluable merely because their declaration is known. Binding `const` is independent of array-element mutability; use a qualified array type for readonly elements.
- **Breaking:** removed builtin functions and builtin-call AST/token variants. `concat`, `str_len` and `str_eq` are ordinary identifiers; their `@` spellings are rejected. Standard-library implementations are future work; the historical `@concat` wrapper is retired and samples/editor highlighting use ordinary calls.
- Type bindings now require explicit `comp` during analysis, including local inline types and type aliases. Local aliases follow declaration order and cannot reuse build-plan file names or their roots. Local compile-time type identities remain private to lexical lookup. Collection and name resolution reject invalid stage ordering and repeated attempts; name errors carry structured source locations.
- LSP semantic highlighting recognizes `pub` and `comp` as declaration keywords, including per-binding modifiers in multi-variable declarations.
- **Breaking:** unified declarations use `[pub] [const | comp] name (: Type = | :=) expression;`. Type definitions and aliases now use `Name: type = ...;` with a required semicolon and parse as variable bindings with arbitrary expression initializers (`TypeValue` for explicit type syntax; ordinary expressions for names and calls, with semantic discrimination later); the legacy `type Name ...` parser route is removed. Each multi-binding retains independent modifiers and types, with the final `:=` introducing the shared initializer list. Typed declarations without initializers remain supported for compatibility. Parser tests, samples, and parsing/agent references follow the new syntax; arbitrary compile-time evaluation remains unfinished.
- Builtin numeric type variants and canonical source names use bit widths (`Int32`, `int32`); bare `int` and `uint` denote 32-bit types. The current parser rejects legacy `@`-prefixed types.
- The current parser uses restricted postfix `&`/`*`, direct indexing and slicing (`arr[i]`, `arr[a..b]`), and `.len` instead of legacy dotted access forms.
- Aligned language references, roadmap baselines, repository instructions, and agent guides with the implemented syntax and explicit unfinished compiler boundaries.
- Moved verified syntax observations into `docs/parsing/`, separate from language-design proposals. Filed analysis gaps as GitHub issues; `docs/agents/issues/README.md` links to them without duplicating report bodies.
- Replaced the legacy driver, CLI, and library entry points with the unfinished build-file pipeline from `refactor/modules-and-generic-functions`. `build`, `run`, and `test` do not work yet; the `AnalyzedProject → Ir` pipeline is represented by architecture sketches, not executable passes.
- Replaced obsolete typed-AST/flat-IR lowering bodies with coherent SSA and backend API sketches; no direct AST-to-LLVM fallback or compatibility `IrProgram` is retained.

### Fixed
- Parse `void` type values through the ordinary type-expression path so calls consume their arguments correctly.
- Reject known-zero remainder divisors independently of the dividend and diagnose inclusive slice-bound normalization overflow without panicking. Failed recursive specialization groups discard newly cached dependents; template-dependent flow obligations are checked on concrete instances.
- Compile-time alias dependencies use an iterative stack, preserving canonical identities and cycle diagnostics while resolving the 8,000-alias regression on a 2 MiB stack.
- Migrated all 15 sample programs to current syntax while preserving their operations and numeric widths; output remains intended/unverified. Fibonacci benchmark results now use `int64` so `fib(50)` fits, and linked-list cleanup is registered after constructing the list.
- Aligned LSP highlighting with the current tokenizer/parser: bare bit-width types, `const` bindings, `.len`, declaration contexts, and decimal-prefixed floats; removed legacy `var`/`import` keyword and `@`-type highlighting.
- Migrated parser identifier handling to string-based `Identifier` tokens and qualified identifier expressions; type declarations consume the metatype builtin, and member accesses consistently use `accessed_identifier`.
- Driver pipeline/lowering sketches now return explicit not-implemented errors instead of panicking.

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
