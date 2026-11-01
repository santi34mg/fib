# Tests and validation

## Type-owned comp function members (2026-10-31)

Full offline workspace tests and strict all-target Clippy passed with both
`--no-default-features` and `--all-features` (LLVM 21 selected with
`LLVM_SYS_211_PREFIX=/usr/lib/llvm/21`). Formatting and whitespace checks passed.
`tests/comp_members.rs` covers parser failures, explicit receiver mutability,
storage exclusion, callbacks, fixed helpers, explicit adapters, recursive/generic
member calls, cross-file/alias lookup, nominal compatibility, capture rejection
and independent annotation validation. Its native test checks one-byte `Dog`
layout, mutable/readonly receivers and indirect callbacks. The sample sweep
compiles and runs `samples/comp_members.fib`, checking `woofwoofwoof` output and
the stateless `PageAllocator` adapter. This implements function members; general
function expressions and non-function member initializers remain unsupported.

## Release archive packaging (2026-10-31)

`fib-dist` built `fib-0.2.0-linux-x86_64.tar.gz` and its SHA-256 checksum using
optimized installed binaries. The archive contains `fibc`, `fib-lsp`, all manifest
std sources, the matching sysroot manifest, license/changelog and runtime dependency
information. Its automatic extraction test compiled hello-world outside the checkout
with no sysroot override, checked exact output/exit status, and launched LSP help.
External checksum verification passed. Wrong-tag and existing-output invocations
failed explicitly before rebuilding or overwriting artifacts.

Both full workspace test configurations passed, including native/std/sample execution.
Formatting, whitespace, all-target checks, strict Clippy in both feature configurations
and dependency policy checks passed. The release workflow uses read-only build jobs
and a write-enabled upload job only for published releases; PR/manual builds retain
Actions artifacts. GitHub-hosted execution and release upload remain unverified locally.

## 0.2.0 release preparation (2026-10-31)

Workspace packages and Cargo.lock, the installed std manifest and VS Code package
metadata now identify 0.2.0. The compiler-declaration revision remains 4. Release
notes describe migration requirements and the remaining implementation boundaries;
the separate Zed extension retains its independent version.

Validation passed after the version changes:

- `cargo fmt --check` and `git diff --check`.
- Offline locked no-default-features check and full workspace tests.
- Offline locked all-features workspace tests with LLVM 21, including native
  acceptance, all top-level samples and source std execution.
- Offline locked strict all-target Clippy in both feature configurations.
- VS Code launcher tests (`node --test` in `lsp/editors/vscode`): two passed.
- The Rust installer built optimized compiler/LSP binaries and installed matching
  sources into a temporary prefix. Installed sources/manifest matched the checkout.
  From outside the checkout, with `FIB_SYSROOT` unset and no `--sysroot` argument,
  the installed compiler discovered std 0.2.0, compiled a copied hello-world sample
  and produced the expected output with exit status zero.

These are local validation results, not a claim that remote CI or publication has
completed. No tag, GitHub release or binary upload was created during preparation.

## Prefix containers and byte sentinels (2026-10-31)

The workspace all-features and no-default-features test suites pass. Strict
all-features/all-target Clippy, formatting and whitespace checks pass. Added
parser and analyzer regressions cover prefix composition, unchanged literal
indexing/slicing, readonly termination and forbidden guarantee-adding casts.
`tests/programs/sentinels.fib` lowers with installed std and executes against C
helpers, covering literals, global/local and partially initialized storage,
nested arrays/structs, copies, generic specialization, empty strings and nonzero
byte sentinels. The final sentinel-initialization loop and uninitialized
multi-binding cases were rechecked with the targeted native acceptance tests.
Sentinel support currently requires uint8 elements and sentinel literals 0–255;
this does not implement arbitrary typed Zig sentinels or sentinel-asserting slice
expressions. Existing array/slice bounds and lifetime limitations remain.

## Canonical cross-file linkage (2026-10-30)

All 881 all-features tests and all 807 no-default-features tests pass across the
workspace. Strict all-target Clippy passes in both configurations; formatting and
whitespace checks also pass. `tests/linkage.rs` adds eight all-features acceptance
tests (six without LLVM), including relocated/renumbered/reordered plans, aliases
and dependency diamonds, coexisting package versions, Unicode normalization and
collisions, canonical generic/nominal/recursive/callback type identities, generic
local nominal types, foreign/entry collisions and native C linkage. The frontend
normalization regression also covers combining marks introduced by case folding.
See the [completed linkage contract](../../roadmap/00-cross-module-linkage.md).

## Workspace crate split (2026-10-30)

All 870 LLVM-enabled tests and all 798 no-default-features tests pass across
workspace packages and root integration tests. Strict all-target Clippy passes
in both configurations, as do formatting and whitespace checks. The frontend's
isolated 707-test suite also passes with `cargo test -p fib-frontend`.
Default workspace members include every compiler package; use `--workspace` for
explicit full validation. See [crate ownership](../../../crates/README.md) and
[warm rebuild measurements](../../rust-build-performance.md).

## Function-pointer fixture repair (2026-10-29)

All eight previously failing function-pointer executable tests now pass after
repairing their fixtures: statement semicolons, explicit main functions, current
enum declarations/variant access, readonly byte slices, postfix address-of,
consistent numeric operand types, and correct arithmetic expectations. The
struct-argument case now passes the complete callback-bearing struct. No compiler
implementation change was needed. This supersedes the fixture failures in the
dated records below.

The unfiltered offline all-features suite passes: 779 library tests, four LSP
unit tests, 44 executable tests, eight LSP integration tests and 23 std integration
tests. Formatting and strict all-target LLVM Clippy also pass.

Opt-in compiler and framed-stdio LSP benchmarks live in `benches/performance.rs`.
See the [performance report](../../performance.md) for commands, measurement
scope, results, and optimization candidates. They are not timing gates.

The [language specification](../../language-specification.md) is implemented for prelude names, casts, shallow readonly views,
warned pointer field access and exhaustive switches. Concrete generic functions,
multiple source files and fixed-array values execute. Package-rooted lookup, scoped
visibility, installed std sources and Linux x86-64 syscalls execute. Recursive source
discovery remains deferred. Checked runtime indexing/slicing and constant-initialized
globals have structural and isolated executable coverage.

## Process entry validation (2026-10-29)

Direct computed-type follow-up: analyzer regressions cover alias-free comp calls
in parameters, returns, locals, globals, fields, enum payloads, casts, callbacks
and container elements, plus rejection of runtime type functions and non-type
results. The process-entry sample uses a direct Result return annotation and
direct computed-type variant construction; executable cases check its 42/0/1
exit statuses with controlled arguments and environments.

Generic-map follow-up (2026-10-29): the full no-default suite passes with 764
library tests and 12 std integration tests (LSP counts unchanged). LLVM 21 passes
776 library tests and all 23 std integration tests. New tests cover computed
signature annotations and malformed syntax, nominal specialization identity,
rejected map/equality type mismatches, scalar and struct keys/values, supplied
equality during replacement, growth, lookup and cleanup/reuse. The environment
tests run against `Map(Bytes, Bytes)`. The 36 other executable tests also pass
with the eight previously failing function-pointer fixtures excluded.
Formatting and strict LLVM Clippy pass.

The full no-default suite passes: 763 library tests, four LSP unit tests, seven
LSP integration tests and 11 std integration tests. LLVM 21 library tests pass
(775). All 21 LLVM std integration tests pass across the full 20-test run and
the added raw-environment-vector test. New coverage includes optional entry
parameters, rejected signatures, Result status mapping and recursive main calls,
empty/Unicode arguments, empty environments, first-equals splitting, duplicate
replacement, malformed entries, non-UTF-8 values, map growth/replacement and
cleanup/reuse. Formatting and strict LLVM Clippy across all targets pass.

The broader LLVM executable run passes 36 tests and retains the eight existing
function-pointer fixture parsing failures recorded below. This is not a green
unfiltered LLVM checkout suite.

## Indexing and global storage validation (2026-10-28)

Formatted-output follow-up (2026-10-28): the full no-default suite passes (762
library tests, four LSP unit tests, seven LSP integration tests and ten std
integration tests). All 17 LLVM 21 std integration tests pass. Output coverage
checks exact bytes/counts, full-range integers, booleans, empty and NUL-containing
strings, escaped braces, invalid formats/counts before output, descriptor errors,
and scripted EINTR, short-write and zero-progress behavior. Formatting and strict
LLVM Clippy across all targets pass. The memory tests now include the I/O module.

Allocator follow-up (2026-10-28): the unfiltered no-default suite passes (760
library tests, four LSP unit tests, seven LSP integration tests, eight std
integration tests). LLVM 21 library tests pass (772), as do all 12 std integration
tests, including native allocator/sbrk execution, failure under RLIMIT_DATA,
overflow, alignment, reuse, splitting/coalescing and zero-sized/qualified types.
The executable suite passes 36 tests; eight pre-existing function-pointer fixtures
fail parsing on missing semicolons. Formatting and strict LLVM Clippy pass.
This supersedes the allocator-related failure status in the older record below.

- No-default library tests: 755 passed with three std-source-dependent tests
  filtered out. LLVM 21 library tests: 767 passed with the same exclusions.
  The excluded tests are `checked_and_unchecked_places_have_distinct_null_guard_ir`,
  `direct_enum_payloads_and_unit_values_lower_without_llvm`, and
  `struct_storage_sample_lowers_to_verified_memory_ir` in `driver::test`.
- Five isolated executable tests cover checked reads/writes/slices and traps,
  initializer relocations and enum layouts, evaluation order, the new sample,
  and shared global storage across files. Intrinsic cases use a minimal installed
  source package; ordinary storage cases disable std.
- The unfiltered no-default library run has those three failures. The unfiltered
  executable run passes the five isolated tests and fails 38 std-dependent tests:
  the existing allocator draft in `library/std/memory.fib` refers to unresolved
  `sizeof`. These runs do not establish a green full-checkout suite.
- Formatting and strict Clippy pass with and without LLVM. Value-returning
  `comp fn` calls deliberately report "not implemented"; definition checking and
  existing bounded type-returning evaluation have coverage.

Unit tests live beside implementation under `src/`. `tests/executable.rs` is an
LLVM-feature-gated acceptance suite requiring LLVM 21 and host `cc`. Its fixtures
include `tests/programs/return_42.fib`, `conditionals.fib`, and the samples
`builtin_scalars.fib`, `scalar_loops.fib`, `struct_storage.fib` and
`enum_runtime.fib`, `enum_payloads.fib` and `defer.fib`. Other samples have parser coverage without an execution claim.
The runnable `example-project/` is copied into a temporary directory for both
no-default frontend/IR validation and an LLVM-enabled CLI build-file/exit-0 test.
Both tests copy the build file and its two sources. Plan assertions check file
membership, the `main` entry and the `demo` output; execution covers all nine
demonstrations, including cross-file calls and aggregate/generic behavior.

## What tests actually establish

- `frontend/tokenizer/test.rs`: token kinds/values, malformed literals and positions.
- `frontend/parser/test.rs`: AST shapes, precedence, current syntax and diagnostics.
  `samples_parse_with_current_frontend` discovers and parses every top-level
  `samples/*.fib` file; this is not semantic or runtime sample coverage.
- [`frontend/analyzer/test.rs`](../../../crates/fib-frontend/src/frontend/analyzer/test.rs): lexical
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
`frontend::analyzer::test::name_resolution` in `crates/fib-frontend/src/frontend/analyzer/test.rs`.
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

## Native language server validation (2026-10-24)

The Rust `fib-lsp` shares compiler tokens, source overlays, parsing, collecting
analysis and build planning. The existing negative type/flow fixture helpers
also assert that collecting analysis retains their expected diagnostic messages.
Additional analyzer cases cover independent failures, blocked dependencies,
scopes, aliases/cycles, warnings and bounded generic specialization.
`frontend::tooling` tests cover semantic tokens, UTF-16/CRLF/multiline ranges,
syntax failures and explicitly unavailable sources.

Verified in this checkout:

- No-default suite: 728 library tests, four native-server unit tests and five
  framed-stdio LSP integration tests passed.
- LLVM 21 suite: 739 library tests, four server unit tests, 28 executable
  acceptance tests and five LSP integration tests passed.
- Formatting, no-default all-target checks and strict Clippy passed.
- Zed's four native launcher tests and the VS Code launcher test file passed.

Protocol tests cover initialization/shutdown, multiple diagnostics, unsaved
sources/build files, cross-file planning, current document versions, token
requests, diagnostic clearing, Unicode, disk dependency watches and source
membership changes. No interactive editor smoke test was performed. Parsing
remains first-error-per-file; failed generic instances are checked strictly and
collected at their callers, and dependent regions remain blocked until fixed.

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

### Direct payload and unit normalization validation (2026-10-28)

All 519 analyzer tests passed, including direct payload typing, generic unit
specialization, constructor rejection and independent switch-binding annotation
validation. Formatting, checks with and without LLVM, and strict no-default
Clippy (`--all-targets -- -D warnings`) passed.

A temporary Rust validation harness checked 29 fixtures through verified IR,
then compiled, linked and executed all 29 with LLVM 21. Its temporary sysroot used
baseline compiler-owned std modules, the migrated Result implementation and
direct-payload OS Status. Coverage included `enum_payloads.fib`, generic void
switch bindings, direct nested enums, pointers and function payloads, zero-length
array payloads, source evaluation order, and integer/float payload alignment for
all supported widths. Scalar, struct, loop, defer and syscall samples also passed.

Full-checkout runs reached 752/755 passing no-default library tests and 763/766
passing LLVM library tests. Three std-dependent driver tests failed on unresolved
`sizeof` in the concurrent allocator work in `library/std/memory.fib`; Cargo
stopped before integration suites. Strict LLVM Clippy reported two warnings in
the concurrent function-pointer field-address implementation (`clone_on_copy`
and `needless_borrows_for_generic_args`). Those edits were retained. These results
do not claim that the full current checkout is green.

## Defer validation (2026-10-27)

`cargo test --offline --no-default-features defer` passed all 27 matching tests.
The LLVM-enabled `defer_blocks_execute_on_lexical_exits` test passed 14 compiled programs, including
`samples/defer.fib`. Coverage includes reverse-order
cleanup, values read at exit, block-local declarations, normal loop completion,
break/continue, nested loops, both return paths, implicit void return, pointer and
aggregate storage, deferred initialization, switches, generic instances and
aggregate return capture. Strict LLVM-enabled Clippy passed with
`--all-targets -- -D warnings`.

The full no-default library run had 721 passes and 19 failures; the LLVM library
run had 731 passes and 20 failures. These runs used `RUST_MIN_STACK=16777216`
after the default test stack overflowed in the generic-expansion regression.
Failures remain in the checkout's string/build-API, C ABI and older rejection
expectations. The full executable suite had 28 passes and one failure in
`current_capabilities_project_builds_from_build_file_and_runs`, reporting an
incompatible readonly string argument to the build API. The full suites are not
green; the passing defer checks do not imply otherwise.

## Test-failure repair (2026-10-27)

Focused parser (113), tokenizer (53), no-default backend (10), LLVM backend
(21), plan (3) and LSP integration (5) tests passed during repair. Generic
expansion now has explicit 2 MiB-stack regression tests: runaway expansion
reports a diagnostic after 16 active specializations, while cached/same-instance
recursion remains valid. Both stack regressions passed without an increased
`RUST_MIN_STACK`.

Build API fixtures use explicit `"text"[..]` byte slices rather than implicit
array-to-slice conversion. Old string-primitive and unsupported-C-ABI assertions
were updated to the current contracts. Both no-default and all-features
`cargo check --offline` passed, as did formatting. Final full-suite validation
is blocked by the example project's removed `features.fib`, still referenced by
the driver fixture's `include_str!`; its `main.fib` is also empty. These concurrent
example changes were not reverted, and the full suites are not claimed green.

## Installed std and syscalls validation (2026-10-27)

This later record supersedes the earlier same-day full-suite limitations above.
The capability fixture now lives under `tests/programs/capabilities`, independently
of the evolving user-facing example project.

- `cargo test --offline --no-default-features`: 750 library tests, four LSP binary
  unit tests, six framed-stdio LSP integration tests and seven std integration
  tests passed, without an increased stack size or an installed sysroot.
- LLVM 21 `cargo test --offline --all-features`: 761 library tests, four LSP binary
  unit tests, 29 executable acceptance tests, six LSP integration tests and ten
  std integration tests passed.
- No-default and LLVM checks, formatting and strict Clippy passed. The LLVM
  commands use `LLVM_SYS_211_PREFIX=/usr/lib/llvm/21` in this environment.
- Std coverage includes package-relative dependency environments, normalized
  names/collisions, mixed source/intrinsic modules, canonical Result identities,
  prelude exports/collisions, private pointer conversion helpers, version failures,
  no-std mode, syscall IR corruption and rejected build-time syscall execution.
- Native coverage includes raw syscall arities, typed errors, output, short reads
  and EOF, anonymous/file-backed mappings, cleanup, register constraints and
  execution after LLVM O2 optimization. Non-Linux/non-x86-64 emission is rejected.
- The previous installer installed binaries and sources under a temporary prefix.
  The installed compiler built and ran `samples/syscalls.fib` without an explicit
  sysroot, including after relocation. `--no-std` compiled a program returning 42
  with `FIB_SYSROOT` pointing to a nonexistent directory.

General runtime indexing/slicing, recursive source discovery, portable OS
selection and resource ownership enforcement are not established by these tests.

## Change guidance

Direct std-source editing regressions passed with no default features: each of
the five std modules retains its original document ID in one std package, and
the framed LSP test resolves std's own `root` paths while tracking unsaved sibling
edits and ignoring an enclosing invalid application build file. Full no-default
and LLVM-enabled suites passed with seven LSP integration tests and respectively
eight and eleven std integration tests.

The Rust installer (`scripts/install.rs`, Cargo binary `fib-install`) was validated
with a debug installation into a temporary prefix. Its installed compiler built
and ran the syscall sample without an explicit sysroot; installed std sources
matched the checkout. Installer help, formatting and strict no-default Clippy
also passed. The installer builds only `fibc` and `fib-lsp` and obtains their
executable paths from Cargo's JSON artifact messages.

Use `Parser::with_source(Tokenizer::new(source), Path::new("test.fib"), source)`
for disk-free parser tests. Add accepted and malformed syntax cases. For diagnostics,
prefer `is_err()` plus stable category/message/location assertions over full strings.
Keep positive and negative name-resolution coverage for implemented invariants. Keep explicit-error contracts for
unrelated unfinished passes. Do not return empty success to silence tests. Once
lowering works, add sample reproducers plus verified IR/LLVM and link/run coverage. Avoid chasing 100% coverage with tautological lowering tests.
