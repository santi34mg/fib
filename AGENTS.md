# AGENTS.md

## Tooling restrictions

Never use Bash or Python in this project. Do not invoke either interpreter or add
scripts written in either language. Use Rust for project tooling and direct
file-edit tools for changes.

The [language specification](docs/language-specification.md) is implemented for prelude names, casts, shallow readonly views,
warned pointer field access and exhaustive switches. Concrete generic functions,
multiple source files and fixed-array values execute. Package-rooted lookup, scoped
visibility, installed std sources and Linux x86-64 syscalls execute. Recursive source
discovery remains deferred. Checked runtime indexing/slicing and constant-initialized
global storage execute; value-returning `comp fn` execution has explicit scaffolding only.

Fib (`fibc`) is a Rust compiler under reconstruction. Start with the
[current implementation guide](docs/agents/README.md), not historical compiler claims.

## Current checkout

- Working: `crates/fib-frontend/src/frontend/tokenizer.rs` + `tokenizer/`, `parser.rs` + `parser/`,
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
  `crates/fib-lowering/src/lowering.rs` entry point lowers analyzed source plus any synthetic modules;
  the driver validates the selected entry contract and chooses the IR consumer. Program lowering supports
  optional main argument slices/environment maps and void/int32/Result returns
  through an analyzed source startup wrapper; generic `std::map::Map(K, V)` uses
  supplied equality and specializes byte strings for the environment map.
  Computed type annotations work in signatures and declarations.
  multi-file fixed-width integer/float/bool helpers, direct calls, initialized locals, assignment, arithmetic/bitwise/shifts, explicit numeric casts, short-circuit logic, comparisons, nested if/else, while loops, break/continue and returns. It revalidates analysis, prevalidates the
  supported subset (including dead branches), and builds SSA.
  Struct values, partial initialization, stack storage, field/pointer mutation and internal struct/pointer calls now work. Dereferences trap on null; `std::memory::unchecked_deref(pointer)` yields an unchecked place. Enums now support constructors, payload copies, switches, storage and internal calls through LLVM; see [enum semantics](docs/enum-semantics.md). Concrete generic functions, fixed-array values and multi-binding locals execute. Defer statements and blocks execute in reverse order on lexical scope exits, including return, break and continue. Array/slice indexing, indexed assignment and slicing execute with bounds checks. Globals require explicit constant initializers and have program lifetime; ordinary initializer calls are rejected. No `static` modifier or startup initializer is introduced.
- `src/bin/fib-lsp.rs` implements a native stdio LSP sharing compiler tokenization,
  parsing, collecting analysis and build planning. `frontend::source` supplies
  unsaved overlays; `frontend::tooling` produces highlights and diagnostics.
  Collecting reports retain partial annotations and explicitly unavailable files;
  only strict validated `AnalyzedProject` values enter lowering. Build/install
  with `--no-default-features --bin fib-lsp`; see [LSP setup](lsp/README.md).
- Primitive types and `type` lex as ordinary identifiers and resolve through a
  protected implicit prelude. Direct same-family literal cast context and explicit
  numeric/bool conversions share constant/runtime rules. Shallow readonly views,
  warned one-layer pointer field access and exhaustive enum switches are enforced.
  `BuildPlan::packages` records explicit package IDs/source roots/dependency maps;
  scoped visibility uses package/directory regions. Package-rooted lookup resolves
  `root` and dependency environments with ASCII case/hyphen normalization. Recursive
  recursive discovery and ownership/lifetime checking remain deferred.
- `BuildPlan::single_file` loads a named file and chooses an executable beside it.
  Build files have an initial `build_api` semantic namespace and a narrow verified-IR
  path for literal source/entry/output operations. It finalizes deterministic plans
  with root-relative path/name/entry/output validation. General interpretation and
  broader builder orchestration remain unfinished.
- `crates/fib-ir/src/ir/` is owned SSA. The transactional builder allocates blocks/values/slots;
  verification checks scalar constants/calls/arithmetic/unary/casts/comparisons and jump/branch/return/trap/unreachable,
  ownership, signatures, reachability and dominance. Enum operations, constants and switches validate payload signatures, tags and active-variant extraction proofs. Structs, enums, typed pointers,
  stack-slot ownership and aggregate/memory operations are verified. Global storage,
  address relocations and constant slice projections are also verified.
- LLVM 21 emits multi-block SSA with PHIs/backedges, the C 32-bit integer/void entry ABI and internal direct scalar/struct/enum/pointer Fib fastcc ABI.
  ABI classification is per signature; one shared description controls a function
  and calls to it. `emit_llvm(ir, target)` remains the sole public entry point.
  Full production Fib ABI, external C interoperability and broader representations
  remain unfinished.
- `build --single-file PATH` compiles the scalar/control-flow subset through LLVM,
  object and host `cc` linking. The narrow `build --build-file PATH` route also works; default discovery, `run` and `test` error explicitly.
  Old positional-file, include-root, check/emit/link/release flags do not exist.

See [scalar semantics](docs/scalar-semantics.md) for wrapping, casts, traps and IEEE rules. Typed numeric values require explicit conversions; no implicit widening is performed.

Enums use `Variant: Type` for one direct payload value. Named-field payload
shorthand is removed; use an explicit struct type and pass one struct value.
Omitted, void, empty-struct and recursively unit-only struct payloads normalize
to bare variant values, without constructor parentheses. Optional switch bindings
on these variants have type void; other bindings expose the payload directly.
Semantic records retain declared types for substitution/validation and share
`SemanticTypes::enum_payload` for normalized behavior. Runtime unit values use an
empty aggregate; void function returns remain resultless and `*void` stays opaque.

## Standard library

`std::memory::create_page_alloc()` returns a public `Allocator` with context,
allocation and free callbacks. It uses mmap/munmap with private mapping headers,
checked sizes and positive power-of-two alignment (including over-page values).
Zero size fails; null free succeeds. Free requires an original live pointer.
This is separate from the typed sbrk allocator. `AllocError` exposes allocation,
alignment, mapping and release failures; no `NotImplemented` placeholder remains.

`std::formatting` owns tagged `Arg` values, template validation and rendering.
`format(destination, template, values)` writes byte strings into caller-owned
`uint8[]` storage after syntax/count/capacity checks; no allocation or NUL suffix.
`format_to` shares rendering with synchronous writer callbacks. `std::io` retains
literal output, write retries and thin `printf`/`fprintf` adapters; its formatting
types alias `std::formatting`, including the `BufferTooSmall` error variant.

`library/std` contains ordinary Fib sources; `library/sysroot.toml` pins compiler
version and compiler-declaration revision. CLI/build/LSP plans load the installed
sources before parsing. Use `--sysroot library` or `FIB_SYSROOT="$PWD/library"` for
development; `--no-std` retains primitive types without std. Low-level analyzer
fixtures explicitly select their modules. `std::memory` replaces automatic
`memory::` access; explicit namespace aliases remain supported. Install binaries
and sources with `cargo run --no-default-features --bin fib-install -- --prefix PREFIX`. See
[standard-library contracts](docs/standard-library.md).

## Toolchain and commands

The Cargo workspace separates `fib-builtins`, `fib-frontend`, `fib-ir`,
`fib-lowering` and `fib-backend` under `crates/`; root `fibc` retains the facade,
driver, build-file interpretation and binaries. See [crate ownership](crates/README.md).
All packages are default members: root test/check/Clippy commands cover the workspace.
Use `cargo test -p fib-frontend` or `cargo test -p fib-ir` for isolated tests.
Cross-stage analyzer/lowering contracts live in `tests/frontend_lowering.rs`.

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
Frontend-only analyzer tests/helpers live in
`crates/fib-frontend/src/frontend/analyzer/test.rs`. Run one phase with
`cargo test --no-default-features name_resolution::phase_01_files`.
`tests/executable.rs` runs LLVM-enabled compile/link/exit-status acceptance tests. Offline is optional outside restricted environments.
Focus tests with
`cargo test --no-default-features frontend::parser` or `frontend::tokenizer`.
The specification implementation passed 714 no-default library tests, 725
LLVM-enabled library tests and 28 executable acceptance tests, plus checks,
formatting and strict Clippy. See [validation status](docs/agents/tests/README.md)
for dated records and limitations.

CI config runs fmt; no-default check/test; LLVM 21 fmt/check/clippy
(`--all-targets -- -D warnings`)/build/test with all features once. These are configured gates,
not verified green status. Audit/deny/coverage are advisory. The optional local
pre-commit hook additionally makes deny blocking. CI skips docs-only paths.

## Syntax and source guidance

Use bare bit-width types (`int32`, `uint64`, aliases `int`/`uint` = 32-bit),
`x := 1;`, `const x := 1;`, `x: int32;`, `fn f(x: int32) int32 { return x; }`,
`while (condition) { ... };`, `a[i]`, `a[a..b]`, `.len`, postfix pointer operations.
Visibility supports `pub`, `pub(dir)` and `pub(root)`. Bindings use `[pub] [const | comp] name (: Type = | :=) expression;`, including
`comp Name: type = struct { field: int32 };`. Analysis requires explicit `comp`
for type bindings; parsing alone still accepts the unmodified form. Multi-bindings retain independent
modifiers/types and a shared initializer list. Legacy `type Name ...` is removed.
Functions are ordinary identifiers; builtin functions and their `@` forms are removed.
Ordinary metatype parameters and type-value arguments provide generic specialization. `comp fn f(T: type) type { return T; }` explicitly enables bounded type computation;
its function type is `comp fn(type) -> type`. Variadics are removed.
Value-returning `comp fn` declarations have normal type/flow checks, but calls
requiring value computation report an explicit not-implemented error.
Type-level `const *T`, `const T[]` and `const T[N]` qualify elements independently of binding
`const`; qualifiers are layer-local and compatibility does not discard them.
Fixed arrays `T[N]` implicitly form full slices in expected `T[]` contexts,
including declarations, assignments, arguments, returns and aggregate fields.
Conversion borrows backing storage without copying or extending its lifetime;
readonly backing storage cannot form a writable slice. Array literals receive
the expected slice element type. Explicit array-to-slice `as` casts remain rejected.
Legacy `@` types, `var`, `for`, dot-index and imports
are not the current working grammar. Parsing is not semantic/runtime support.
See [parser details](docs/agents/src/frontend/parser.md). Samples use current
syntax and have automatic parser coverage. The installed source std provides `std::result::Result(T, E)`, `std::memory`
intrinsics and Linux x86-64 `std::os` syscalls. General library modules remain pending.

`FileId`/`FileName`/paths live in `plan.rs`; syntax retains point spans and
pending analysis slots. `AnalyzedProject::source_paths` retains plan paths after analysis; there is no
central source-text store. Prefer
`Parser::with_source` for in-memory tests; preserve source context and identities
when implementing missing passes. See [diagnostics](docs/agents/src/diagnostics.md).

## Tests and contribution discipline

- Add tokenizer/parser success and malformed-input tests for syntax changes.
- Analyzer tests combine utility/stub contracts with six inline name-resolution
  specification phases in `crates/fib-frontend/src/frontend/analyzer/test.rs`. The phases pass for
  implemented name resolution; typing/member coverage is in the inline `type_resolution` module, including
  passing `completion_acceptance` and `completion_regressions` cases. Driver/IR/backend tests
  cover the supported scalar/struct/enum/pointer subset and reject unsupported work.
- Never turn unfinished operations into successful empty results. Implement the
  root invariant, then add real positive/negative coverage. Assert diagnostic
  category/substrings/locations instead of brittle full rendered strings.
- Future lowering branches should ship sample reproducers and executable tests
  through the existing harness. All top-level samples are automatically parsed;
  selected scalar, loop, struct and enum samples also have execution coverage. Use `cargo test --no-default-features samples_parse_with_current_frontend`.
- Keep changes focused; preserve user work. Do not chase advisory coverage totals.
- Preserve trunk-based/main and Conventional Commit PR guidance; update
  `CHANGELOG.md` under `[Unreleased]` for behavior changes when in scope.
- Update mirrored agent docs with implementation changes. Clearly distinguish
  working code, representation contracts, sketches, and future policy.
- Keep verified tokenizer/parser behavior in `docs/parsing/`, not as the intended
  language specification. Parser limitations during the refactor are not language
  decisions. Track analysis gaps in GitHub issues, not local report copies.
  Keep only GitHub links in `docs/agents/issues/README.md`.
