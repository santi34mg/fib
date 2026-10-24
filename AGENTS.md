# AGENTS.md

Fib (`fibc`) is a Rust compiler under reconstruction. Start with the
[current implementation guide](docs/agents/README.md), not historical compiler claims.

## Current checkout

- Working: `src/frontend/tokenizer.rs` + `tokenizer/`, `parser.rs` + `parser/`,
  AST construction, lexical scope utilities, diagnostics and raw IR data utilities.
- `analyzer.rs` + `analyzer/` validate plan/unit IDs, collect and annotate declarations,
  and resolve lexical names, file paths, visibility, and compile-time aliases.
  Name resolution validates its annotation slots. The type pass resolves semantic
  types, expressions and owner-based members and validates applicable annotations;
  explicit metatype specialization with concrete body rechecking, bounded `comp fn`
  type computation, independent binding/element mutability, `never` bottom coercion,
  initialization and structured flow validation now run. Numeric literals use arbitrary-precision integer magnitudes and exact decimal-to-IEEE rounding; constant arithmetic shares fixed-width wrapping/IEEE semantics with runtime.
  Analysis annotates the original AST; no
  lexer/analyze/typed_ast modules exist.
- `src/driver.rs` implements parsing, analysis and pipeline orchestration. One
  `src/ir/lowering.rs` entry point lowers analyzed source plus any synthetic modules;
  the driver validates the selected entry contract and chooses the IR consumer. Program lowering supports
  single-file fixed-width integer/float/bool helpers, direct calls, initialized locals, assignment, arithmetic/bitwise/shifts, explicit numeric casts, short-circuit logic, comparisons, nested if/else, while loops, break/continue and returns. It revalidates analysis, prevalidates the
  supported subset (including dead branches), and builds SSA.
  Struct values, partial initialization, stack storage, field/pointer mutation and internal struct/pointer calls now work. Dereferences trap on null; `memory::unchecked_deref(pointer)` yields an unchecked place. Defer, runtime globals, arrays/slices/enums and generic bodies still fail.
- `BuildPlan::single_file` loads a named file and chooses an executable beside it.
  Build files have an initial `build_api` semantic namespace and a narrow verified-IR
  path for literal source/entry/output operations. It finalizes deterministic plans
  with root-relative path/name/entry/output validation. General interpretation and
  broader builder orchestration remain unfinished.
- `src/ir/` is owned SSA. The transactional builder allocates blocks/values/slots;
  verification checks scalar constants/calls/arithmetic/unary/casts/comparisons and jump/branch/return/trap/unreachable,
  ownership, signatures, reachability and dominance. Structs, typed pointers,
  stack-slot ownership and aggregate/memory operations are verified; globals and
  other unsupported families still fail.
- LLVM 21 emits multi-block SSA with PHIs/backedges, the C 32-bit integer/void entry ABI and internal direct scalar/struct/pointer Fib fastcc ABI.
  ABI classification is per signature; one shared description controls a function
  and calls to it. `emit_llvm(ir, target)` remains the sole public entry point.
  Full production Fib ABI, external C interoperability and broader representations
  remain unfinished.
  `backend/lowering/next/` is inactive legacy code, not a supported route.
- `build --single-file PATH` compiles the scalar/control-flow subset through LLVM,
  object and host `cc` linking. The narrow `build --build-file PATH` route also works; default discovery, `run` and `test` error explicitly.
  Old positional-file, include-root, check/emit/link/release flags do not exist.

See [scalar semantics](docs/scalar-semantics.md) for wrapping, casts, traps and IEEE rules. Typed numeric values require explicit conversions; no implicit widening is performed.

## Toolchain and commands

`rust-toolchain.toml` and CI pin 1.98.0 (MSRV policy); bump deliberately together.
Rust 2024 package; default `llvm` enables Inkwell 0.8 / LLVM 21 dependency builds,
with limited working LLVM emission. No-default-features work needs no LLVM installation.

Verified on system Cargo/rustc 1.98.0 (offline cache present):

```sh
cargo check --offline --no-default-features
cargo test --offline --no-default-features
cargo run --offline --no-default-features -- --help
```

The completed semantic-reconstruction pre-commit run passed all blocking checks,
including 677 LLVM-enabled library tests and five executable acceptance tests.
The no-default suite, formatting, checks and Clippy also passed through that hook.
All analyzer tests/helpers live in
`src/frontend/analyzer/test.rs`. Run one phase with
`cargo test --no-default-features name_resolution::phase_01_files`.
`tests/executable.rs` runs LLVM-enabled compile/link/exit-status acceptance tests. Offline is optional outside restricted environments.
Focus tests with
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
Bindings use `[pub] [const | comp] name (: Type = | :=) expression;`, including
`comp Name: type = struct { field: int32 };`. Analysis requires explicit `comp`
for type bindings; parsing alone still accepts the unmodified form. Multi-bindings retain independent
modifiers/types and a shared initializer list. Legacy `type Name ...` is removed.
Functions are ordinary identifiers; builtin functions and their `@` forms are removed.
`comp fn f(T: type) type { return T; }` explicitly enables bounded type computation;
its function type is `comp fn(type) -> type`. Variadics are removed. Type-level
`const *T`, `const T[]` and `const T[N]` qualify elements independently of binding
`const`; qualifiers are layer-local and compatibility does not discard them.
Legacy `@` types, `var`, `for`, dot-index and imports
are not the current working grammar. Parsing is not semantic/runtime support.
See [parser details](docs/agents/src/frontend/parser.md). Samples use current
syntax and have automatic parser coverage; std sources may still be stale.

`FileId`/`FileName`/paths live in `plan.rs`; syntax retains point spans and
pending analysis slots. `AnalyzedProject::source_paths` retains plan paths after analysis; there is no
central source-text store. Prefer
`Parser::with_source` for in-memory tests; preserve source context and identities
when implementing missing passes. See [diagnostics](docs/agents/src/diagnostics.md).

## Tests and contribution discipline

- Add tokenizer/parser success and malformed-input tests for syntax changes.
- Analyzer tests combine utility/stub contracts with six inline name-resolution
  specification phases in `src/frontend/analyzer/test.rs`. The phases pass for
  implemented name resolution; typing/member coverage is in the inline `type_resolution` module, including
  passing `completion_acceptance` and `completion_regressions` cases. Driver/IR/backend tests
  cover the first verified scalar/executable subset and reject unsupported work.
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
