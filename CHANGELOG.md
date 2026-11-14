# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Std failures are reported only through `Result`/`Option`. `std::os` errors
  carry the new `std::os::Errno` alias, `std::os::Status` is
  `Result(void, Errno)`, and `getpid` returns `int32`. `std::os` flag constants
  and `stdin`/`stdout`/`stderr` are explicitly typed; `MAP_FAILED` is removed.
- One allocator interface: `std::memory::Allocator.free` takes `*uint8`, the sbrk
  heap is `heap_allocator()` and `create_page_alloc()` is renamed
  `page_allocator()`. `alloc(T, n)` returns `Result([]T, AllocError)` and
  `free(T, items)` takes the allocated slice and returns `Status`; unknown and
  double frees return the new `AllocError.InvalidFree` instead of being ignored.
- `std::process::Arguments` is `[]const [:0]uint8` and `Environment` is
  `Map(const []uint8, const [:0]uint8)`, so entry strings pass to path syscalls
  through `slice_data`. `std::process::Bytes` is removed. Startup allocation
  failure aborts with a message (status 134) instead of exiting silently.
- `std::map::insert` returns `std::memory::Status`, `get` returns `Option(V)`,
  and `bytes_equal` is replaced by `std::string::eq` (formerly `str_eq`);
  `str_len` is removed in favor of `.len`.
- `std::io::write_all` takes a `Writer` and returns `Result(usize, Errno)`;
  `std::random` and printing share its retry loop. `formatting::Error.Io` and
  `random::Error.Io` carry `Errno`.
- Compiler-declaration revision 6; reinstall std sources with the compiler.

### Removed

- The "implicit pointer dereference in field access" warning. `pointer.field`
  still inserts one checked dereference, now silently.
- The empty `std::path` placeholder module and the `std::io::Reader(T)` scaffold.

### Added

- macOS on Apple Silicon (arm64) builds and runs Fib programs natively:
  - The C ABI classifier implements AAPCS64 alongside System V x86-64:
    homogeneous float aggregates, register-pair composites, 16-byte-aligned
    values and caller-copied large aggregates match clang in both directions.
  - `sysroot.toml` gains `[targets.<family>]` sections that `replace` or
    `exclude` std sources per OS family while module paths keep their logical
    names; the darwin std implements `std::os` over libSystem
    (`library/std/darwin/`) with the Linux API and darwin constant values.
  - The std heap allocator obtains storage through a package-private
    `os::heap_grow` (program break on Linux, mappings on macOS).
  - float128 arithmetic, comparisons and conversions are rejected on Apple
    arm64 with a clear error, since its runtime lacks soft-float routines.
  - CI runs the LLVM test suite on `macos-latest`; Linux-only tests are gated.
- `std::result` and `std::option` helpers: `is_ok`/`is_err`, `is_some`/`is_none`,
  `unwrap_or`, `expect`, `map`, `map_err`, `ok`, `err` and `ok_or`.
- A working `std::vector::Vec(T)` (`init`, `push`, `reserve`, `pop`, `get`,
  `items`, `capacity`, `clear`, `deinit`) over an explicit allocator.
- `std::io::Reader`/`Writer` callback interfaces with `fd_reader`, `fd_writer`,
  `stdin`/`stdout`/`stderr`, `read_full`, `write_all` and `writef`.
- `std::memory::create`/`destroy` for any allocator, `std::memory::span` for C
  strings, `std::os::errno_name`, `std::process::getenv`/`abort` and
  `std::string::index_of`, backed by the package-private
  `std::os_abi::terminated_from_parts` intrinsic.

- Functions returning `never` now lower and execute: definitions, `callconv "C"`
  declarations, direct and indirect calls, and never-typed operands such as
  `x + stop()` or `b || stop()`. A never call is followed by `unreachable` and
  does not run pending defers. `std::os::exit` now returns `never`, so
  `fn main() int32 { std::os::exit(7); }` needs no trailing `return`.
- `std::syscall` names every Linux x86-64 syscall number (0–334, 424–469).
  `std::os` adds typed wrappers for the common POSIX calls (file/directory,
  descriptor, memory, process, time, randomness and socket operations), kernel
  structures (`Stat`, `Timespec`, `Utsname`, …) and flag/signal/errno constants.
  Existing wrappers use the named numbers; `sbrk` errors use `ENOMEM`.
- `null requires a pointer context` diagnostics now suggest a `null as *T` cast.
  For `std::memory::slice_from_parts`/`unchecked_deref`, T comes from the
  expected result type when known (e.g. `null as const *int64`).
- Type-owned struct `comp` function members with explicit receiver types,
  ordinary callback values and fixed helpers. Members add no instance storage
  and cannot be initialized or replaced by instances; interface adapters remain
  explicit.
- Contextual `Self` inside struct type bodies (fields and `comp` members) names
  the struct being defined, including anonymous structs returned by type
  computations such as `comp fn List(T: type) type`. `Self` is reserved.
- Native LSP completion for in-scope parameters/locals and struct fields, including
  partial and unfinished expressions, UTF-16 positions and `.` triggering.
- Build-file IR execution of branches, loops, block arguments and direct helpers,
  with integer/boolean values, computed byte arguments, transactional build effects
  and explicit step/call-depth limits.
- Bare `build` discovers exactly `<cwd>/build.fib` and uses the existing
  interpreted build-file pipeline, with actionable errors when discovery fails.

- `export-symbols --single-file PATH` / `--build-file PATH` exports deterministic
  JSON for unrestricted public top-level declarations, signatures and alias
  targets without requiring an executable entry or LLVM.

### Fixed

- `comp` members of structs returned by type computations (e.g. `Vec(T)`)
  specialize per computed owner: `Vec(int32).push` checks and lowers with
  `T = int32` and `Self = Vec(int32)`. Symbolic member templates are no
  longer lowered.
- Reject radix-prefixed numeric literals without digits (such as `0d.5`) instead of treating them as decimal floats.
- Restore compiler builds after integrating indexed compound-assignment syntax
  with type-owned member traversal; visit both target and value expressions.
- Parse indexed compound assignments for all five arithmetic operators, retaining
  one target and RHS in the AST. Analysis and execution remain unsupported.

## [0.2.0] - 2026-10-31

### Added

- Reconstructed native compilation through analyzed source, verified owned SSA,
  LLVM 21, PIC objects and host `cc` linking. Explicit single-file and narrow
  interpreted build-file routes work; default discovery, `run` and `test` remain pending.
- Fixed-width signed/unsigned 8–128-bit integers, float16/32/64/128 and bool:
  arithmetic, bitwise operations, shifts, numeric/bool casts, comparisons,
  short-circuit logic, helpers, recursion, branches, loops and break/continue.
  Integer arithmetic wraps; invalid division, shifts and float-to-integer casts trap.
- Structs, partial initialization, typed pointers, null-checked dereferences,
  unchecked memory places, direct enum payloads/switches, fixed-array values,
  checked indexing/slicing and constant-initialized global storage with relocations.
- Function values and indirect calls, including mutable callbacks in struct fields.
  Linux x86-64 System V `callconv "C"` supports scalar/pointer and nonempty-struct
  boundaries, register coercions and indirect struct arguments/results. Native
  linking accepts `--c-source`, `-l`/`--link-lib` and `-L`/`--library-path`.
- Concrete generic specialization with body rechecking, computed type annotations,
  bounded `comp fn` type computation, initialization/flow validation, exhaustive
  enum switches and reverse-order lexical defer on normal exits/return/break/continue.
  Value-returning `comp fn` evaluation remains explicit scaffolding.
- Package-rooted lookup, scoped visibility, identity-preserving aliases, protected
  primitive prelude names and canonical cross-file linkage with persistent package
  identities, pinned Unicode normalization and versioned symbols.
- Installed version-matched source std with `--sysroot`/`--no-std`, reusable Result,
  Linux x86-64 syscalls, process arguments/environment and generic maps. Main accepts
  optional argument/environment parameters and void/int32 or supported Result returns.
- Source std typed sbrk allocation/free, aligned mmap page allocation/release,
  checked byte `memset`/`memcpy`, target-derived runtime `core::sizeof(T)`, OS randomness,
  tagged formatting to buffers/writer callbacks, reliable output and explicit string
  equality/length/concatenation helpers. Buffer formatting adds no NUL suffix.
- Native compiler-backed LSP with collecting diagnostics, semantic tokens, unsaved
  overlays, UTF-16 positions, build-file planning and accessible qualified-name completion.
- Rust installer for compiler/LSP binaries and std sources; compiler/LSP benchmark
  tooling with recorded measurements. Automatic top-level sample compile/run coverage
  checks output and exit statuses, alongside frontend, IR, native and std regressions.
- Rust release packaging for Linux x86-64 binaries and matching std, with SHA-256
  checksums and extracted-archive smoke tests. Publishing a GitHub release builds
  and attaches the bundle; manual/PR workflow runs produce Actions artifacts only.
- Experimental `fibc-fib` token/arena/command scaffolding and `std::io::Reader(T)`
  scaffolding. Neither is a working compiler/input abstraction; the Fib compiler
  experiment is outside the sample sweep and still needs API migration.

### Changed

- **Breaking:** prefix container types `[N]T`/`[]T` replace suffix forms. Byte sentinel
  arrays `[N:S]uint8`, slices `[:S]uint8` and pointers `[*:S]uint8` preserve termination;
  `c"..."` has type `const [*:0]uint8`. `openat` requires a terminated path.
- **Breaking:** unified bindings require explicit `comp` for types. Primitive types
  are ordinary protected identifiers; `char` is removed and character literals use uint8.
  Binding constness and layer-local element/pointee constness are independent.
- **Breaking:** enum variants carry one direct payload type; recursively unit-only
  payloads use bare variant values. Named-field payload shorthand is removed.
- **Breaking:** remove `@` builtins/types, variadics, legacy type declarations and
  `extern` (use `callconv "C"`). `std::memory` replaces automatic `memory::` access;
  string helpers live explicitly in `std::string`. Fixed-signature shims handle C varargs.
- Arrays implicitly borrow as full slices in expected-type contexts, preserving
  readonly backing storage; string literals can form readonly byte slices.
- Split compiler stages into workspace crates, retaining the root facade/driver.
  Share immutable collecting-analysis data, reuse verifier graph tables, reduce
  development debug-info costs and avoid duplicate LLVM validation runs.
- Replace the historical flat-IR/typed-AST pipeline and experimental std with the
  current analyzer, transactional SSA, shared lowering and installed source std.
  Compiler-declaration revision is now 4. Historical positional/check/emit/release
  CLI flags are absent; runtime checks are not disabled by a release flag.

### Fixed

- Render readable types/readonly qualifiers and explain invalid slice conversions.
- Preserve const protection through nested inline arrays, field access, addresses
  and borrowed views; check bounds before narrowing indices or writing destinations.
- Preserve package/intrinsic identities and unsaved sibling overlays when editing std.
- Bound active generic expansion, clean failed specialization caches and resolve long
  alias chains iteratively; reject malformed ABI plans without panicking.
- Preserve existing outputs on linker failure and reject source-overwriting outputs.
- Repair current-syntax callback fixtures and migrate all top-level samples to source std.
- Reject malformed character quotes, known-zero remainders and overflowing inclusive
  slice bounds; parse void type arguments through the ordinary type-expression path.

## [0.1.1] - 2026-10-14

### Added
- Slice types (`[]T`), explicit array views and sub-slicing, inclusive and
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

[Unreleased]: https://github.com/santi34mg/fib/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/santi34mg/fib/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/santi34mg/fib/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/santi34mg/fib/releases/tag/v0.1.0
