# Tests and validation

The [language specification](../../language-specification.md) is implemented for prelude names, casts, shallow readonly views,
warned pointer field access and exhaustive switches. Concrete generic functions,
multiple source files and fixed-array values execute. Package metadata/scoped
visibility exist; package-rooted lookup/discovery and runtime indexing/slices remain deferred.

Unit tests live beside implementation under `src/`. `tests/executable.rs` is an
LLVM-feature-gated acceptance suite requiring LLVM 21 and host `cc`. Its fixtures
include `tests/programs/return_42.fib`, `conditionals.fib`, and the samples
`builtin_scalars.fib`, `scalar_loops.fib`, `struct_storage.fib` and
`enum_runtime.fib`. Other samples have parser coverage without an execution claim.
The runnable `example-project/` is copied into a temporary directory for both
no-default frontend/IR validation and an LLVM-enabled CLI build-file/exit-0 test.

## What tests actually establish

- `frontend/tokenizer/test.rs`: token kinds/values, malformed literals and positions.
- `frontend/parser/test.rs`: AST shapes, precedence, current syntax and diagnostics.
  `samples_parse_with_current_frontend` discovers and parses every top-level
  `samples/*.fib` file; this is not semantic or runtime sample coverage.
- [`frontend/analyzer/test.rs`](../../../src/frontend/analyzer/test.rs): lexical
  scope/identity utilities, six inline `name_resolution` specification phases,
  and `type_resolution` acceptance/annotation-corruption cases. All
  analyzer tests and helpers live in this one file. The six phases pass for the
  implemented name resolver. Separate type tests establish semantic typing and
  members; neither suite establishes execution.
- `diagnostics.rs`: parser conversion and rendering/line extraction.
- `driver/test.rs`: scalar/aggregate/control-flow SSA, loops, enum switches,
  scopes/returns, source paths, invalid plans and subset rejection.
- `ir/test.rs`: raw records, transactional construction, scalar/aggregate/memory
  invariants, enum active-payload proofs, ownership/signature/dominance checks.
- `backend/lowering/test.rs`: malformed input, scalar calls and LLVM target validation.
- `backend/lowering/abi/test.rs`: classification without LLVM and invalid boundary maps.
- `backend/lowering/cfg_test.rs`: executable PHIs/backedges/parallel edges, comparisons and per-function ABI dispatch.
- `tests/executable.rs`: CLI exits 0/42/127, object linking, unsupported source, source protection and linker failure preserving output.

## Phased name-resolution specification

The six phases are inline modules under
`frontend::analyzer::test::name_resolution` in `src/frontend/analyzer/test.rs`.
Their fixtures and inspection helpers are in the same file; there is no separate
`analyzer/name_resolution/` directory.

Negative cases require relevant diagnostics and reject generic not-implemented
errors. Positive cases check semantic identities and annotations, not just a
successful result. Source spellings/spans and later-pass annotations are checked
for preservation. Tests also cover structured diagnostic locations, stage ordering,
failed-pass retry rejection, and scope cleanup on errors.

Fixtures parse named in-memory units and supply explicit build plans. Direct AST
fixtures cover represented but unparsed public-function flags and assignment
variants; direct contexts isolate expression/statement traversal.

| Phase / Cargo filter suffix | Related entities | Passing | Failing |
| --- | --- | ---: | ---: |
| `phase_01_files` | Registration, declaration identities, API ordering, diagnostics | 26 | 0 |
| `phase_02_locals` | Parameters, locals, multi-bindings, scope lifetime | 32 | 0 |
| `phase_03_references` | File fallback, functions, qualified names, access checks | 20 | 0 |
| `phase_04_aliases` | Compile-time aliases, canonical targets, cycles, file collisions | 30 | 0 |
| `phase_05_types` | Explicit comp types, local type aliases, separate member namespaces | 32 | 0 |
| `phase_06_traversal` | Expression/statement children, payloads, labels, scope cleanup | 96 | 0 |

Run each phase independently, or run the entire specification:

```sh
cargo test --offline --no-default-features name_resolution::phase_01_files
cargo test --offline --no-default-features name_resolution::phase_02_locals
cargo test --offline --no-default-features name_resolution::phase_03_references
cargo test --offline --no-default-features name_resolution::phase_04_aliases
cargo test --offline --no-default-features name_resolution::phase_05_types
cargo test --offline --no-default-features name_resolution::phase_06_traversal
cargo test --offline --no-default-features name_resolution
```

For local value declarations, resolve **all** initializer expressions before
registering **any** new bindings. Thus `x := x;` needs an existing binding for its
RHS; `a, b := 1, a;` cannot use the new `a` in its shared initializer list.
File declarations and file aliases support forward references; local aliases
follow declaration order. Type bindings require explicit `comp`, and local aliases
cannot reuse build-plan file names/roots. Nested redeclarations of enclosing function
parameters/locals are rejected; sibling scopes may reuse spellings. Initialization
validity, typing, and source-label/generic syntax are outside this specification.

## Type-resolution acceptance

`type_resolution` covers canonical/nominal identities, recursion, contextual
literals, expressions, places, calls/returns, aggregate construction, members,
bounds, patterns, multiple files, source diagnostics and completion corruption.
All analyzer fixtures/helpers remain in the inline test file. Acceptance cases
were run red before their corresponding implementation was added.

The pure compatibility tests use already resolved semantic IDs. Literal/context,
place and cast tests exercise resolvers instead; compatibility does not infer or
mutate syntax. Empty type workspaces and standalone helpers have real positive
contracts. Generic instance caches and call annotations receive independent
corruption checks. `completion_acceptance` covers the six former pending cases;
`completion_regressions` covers substitution, nominal identities, bounded type
computation, partial initialization, branch joins, loops, enum switches and defer
ordering. Initialization/return completeness use the complete `analyze` seam;
type-only fixtures still exercise `resolve_types` independently.

## Specification implementation validation (2026-10-24)

The implementation passed:

- `cargo test --offline --no-default-features`: 714 library tests.
- `LLVM_SYS_211_PREFIX=/usr/lib/llvm/21 cargo test --offline --all-features`:
  725 library tests and 28 executable acceptance tests.
- Formatting, no-default and LLVM-enabled checks, strict Clippy in both feature
  configurations, and the LLVM-enabled all-features build.

New coverage checks literal-cast context/ranges, numeric/bool casts across every
numeric runtime representation, NaN/signed zero, readonly views and nested-cast
restrictions, implicit-pointer warnings/null traps, RHS-first assignment,
exhaustive unreachable switches, ordinary prelude names/type brackets, generic
recursion/loops/aggregates, multi-file entry identity, fixed-array values/lengths,
IR array corruption, and package membership/scoped visibility/re-exports.
`specification_runtime.fib` is both parser-covered and compiled/executed.

Package-rooted discovery and runtime indexing/slices remain deferred; these runs
do not establish support for them.

## Validation records

The following results are historical milestone runs, not current test totals.
The documentation audit on 2026-10-24 inspected source and test coverage without
rerunning compiler suites. Test presence describes scenarios covered; it does not
establish a new passing run.

### Semantic reconstruction (2026-10-21)

The completed semantic-reconstruction pre-commit run passed all blocking checks,
including the no-default suite, formatting, check/Clippy, **677 LLVM-enabled library
tests** and **five executable acceptance tests**. This run includes the final
review fixes for generic template flow, recursive cache rollback and
return/literal/cast validation, plus the `void` type-value parser regression.
Earlier terminal-access restrictions were lifted for the requested commits.

```sh
cargo check --offline --no-default-features
cargo test --offline --no-default-features
cargo fmt --check
cargo clippy --offline --no-default-features --all-targets -- -D warnings
cargo check --offline
cargo test --offline
cargo clippy --offline --all-targets -- -D warnings
```

The LSP's previously verified 11 tests use `node --test` from `lsp/`. Those
highlighting/protocol tests do not establish a working compiler pipeline.
The scalar/struct/enum/pointer/control-flow subset runs through verified SSA and LLVM to
linked executables. Test-first changes were observed failing at their missing
phase before implementation. Defer, runtime globals and full production Fib ABI remain unfinished.
Loop acceptance covers zero/multiple iterations, nested loops, break/continue,
returns, short-circuit conditions, and all numeric widths plus bool. Executable
loop cases have a five-second execution timeout. Build-file CFG rejection is
checked before operations mutate the plan builder.
Scalar coverage now includes full-width numeric literals, exact IEEE constant
evaluation, wrapping integer operations, explicit conversions, scalar fastcc
helpers/PHIs, short-circuit booleans and guarded runtime traps.

### Loop milestone (2026-10-24)

Formatting, check, tests and all-target Clippy with warnings denied passed both
without LLVM and with LLVM 21 (`LLVM_SYS_211_PREFIX=/usr/lib/llvm/21`), offline.
The no-default suite passed 688 library tests; the LLVM-enabled suite passed
695 library tests and 16 executable acceptance tests. The parser sample sweep
includes `scalar_loops.fib`.

### Struct/storage milestone

The struct/storage run passed 700 no-default library tests, 708 LLVM-enabled
library tests and 21 executable tests, with formatting/check/Clippy in both modes.
Struct acceptance covers copies, partial-field reads/initialization, nested fields,
address-taken scalars, aliases, recursive calls/types, empty structs, every numeric
field width, source evaluation order and readonly/unchecked-place rejection.
IR tests check malformed projections/load-store types, orphan/foreign slots,
by-value cycles and failed-builder rollback. Backend CFG coverage combines struct
insert/extract, parallel aggregate PHIs and memory round trips. Source null access
traps; unchecked guard omission and later checked indirection are inspected in IR.

### Enum coverage present in source

IR tests cover enum signatures/constants, discriminants, finite layouts, active
payload proofs and switch successors. Lowering and executable tests cover
constructors, payload copies, helper calls, nested struct/enum storage, pointers,
numeric payload widths/alignment and once-only source evaluation order. Backend
coverage includes enum constants and switch PHIs/backedges. This inventory is
source evidence, not a new validation run or test count.

## Change guidance

Use `Parser::with_source(Tokenizer::new(source), Path::new("test.fib"), source)`
for disk-free parser tests. Add accepted and malformed syntax cases. For diagnostics,
prefer `is_err()` plus stable category/message/location assertions over full strings.
Keep positive and negative name-resolution coverage for implemented invariants. Keep explicit-error contracts for
unrelated unfinished passes. Do not return empty success to silence tests. Once
lowering works, add sample reproducers plus verified IR/LLVM and link/run coverage. Avoid chasing 100% coverage with tautological lowering tests.
