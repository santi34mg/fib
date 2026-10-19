# Tests and validation

There is no root `tests/` directory or `tests/e2e.rs`. This legacy documentation
path is retained for test guidance. Tests live beside implementation under `src/`.
No `compile_project`, `get_typed_err`, source-override compilation helper, sample
runner, or integration target should be assumed.

## What tests actually establish

- `frontend/tokenizer/test.rs`: token kinds/values, malformed literals and positions.
- `frontend/parser/test.rs`: AST shapes, precedence, current syntax and diagnostics.
  `samples_parse_with_current_frontend` discovers and parses every top-level
  `samples/*.fib` file; this is not semantic or runtime sample coverage.
- [`frontend/analyzer/test.rs`](../../../src/frontend/analyzer/test.rs): lexical
  scope/identity utilities, name resolution, and unfinished typing contracts,
  and the inline `name_resolution` module with six specification phases. All
  analyzer tests and helpers live in this one file. The six phases pass for the
  implemented name resolver; they do not establish semantic typing or execution.
- `diagnostics.rs`: parser conversion and rendering/line extraction.
- `driver/test.rs`: pipeline and lowering stubs explicitly fail.
- `ir/test.rs`: raw representation/access/display contracts and unfinished API failure.
- `backend/lowering/test.rs`: backend sketch boundaries do not fabricate success.

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
| `phase_04_aliases` | Compile-time aliases, canonical targets, cycles, file collisions | 29 | 0 |
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

## Local verification (2026-10-19)

The full no-default-features library suite passes **413 tests**, with zero binary
tests and zero doctests. The six name-resolution phases contribute **235 tests**.
No cases are ignored. Typing/member/generic and driver/IR/backend failure contracts
remain explicit tests of unfinished work.

Verified commands:

```sh
cargo check --offline --no-default-features
cargo test --offline --no-default-features
cargo fmt --check
cargo clippy --offline --no-default-features --all-targets -- -D warnings
```

LLVM/default-feature checks were not run. These results do not establish a green
CI run or successful complete analysis, lowering, or execution.

## Change guidance

Use `Parser::with_source(Tokenizer::new(source), Path::new("test.fib"), source)`
for disk-free parser tests. Add accepted and malformed syntax cases. For diagnostics,
prefer `is_err()` plus stable category/message/location assertions over full strings.
Keep positive and negative name-resolution coverage for implemented invariants. Keep explicit-error contracts for
unrelated unfinished passes. Do not return empty success to silence tests. Once
lowering works, add sample reproducers plus verified IR/LLVM and link/run coverage. Avoid chasing 100% coverage with tautological lowering tests.
