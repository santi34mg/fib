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
  scope/identity utilities, pending annotations, unfinished AST/typing contracts,
  and the inline `name_resolution` module with six specification phases. All
  analyzer tests and helpers live in this one file. Failing specification tests
  expose unfinished behavior; passing utility tests do not establish a completed
  resolver.
- `diagnostics.rs`: parser conversion and rendering/line extraction.
- `driver/test.rs`: pipeline and lowering stubs explicitly fail.
- `ir/test.rs`: raw representation/access/display contracts and unfinished API failure.
- `backend/lowering/test.rs`: backend sketch boundaries do not fabricate success.

## Phased name-resolution specification

The six phases are inline modules under
`frontend::analyzer::test::name_resolution` in `src/frontend/analyzer/test.rs`.
Their fixtures and inspection helpers are in the same file; there is no separate
`analyzer/name_resolution/` directory.

The suite is tests-only: it does not implement name resolution, suppress failing
cases, or expect resolver panics. Negative cases require a relevant diagnostic
substring and reject generic not-implemented errors. Positive cases check semantic
identities and annotations, not just a successful result. Source spellings/spans
and later-pass annotations are checked for preservation.

Fixtures parse named in-memory units and supply explicit build plans. Direct AST
fixtures cover represented but unparsed public-function flags and assignment
variants; direct contexts isolate traversal from unfinished function signatures.

| Phase / Cargo filter suffix | Related entities | Passing | Failing |
| --- | --- | ---: | ---: |
| `phase_01_files` | Files, registration, declaration identities and visibility flags | 18 | 5 |
| `phase_02_locals` | Parameters, locals, multi-bindings, scope lifetime | 0 | 32 |
| `phase_03_references` | File fallback, functions, qualified names, access checks | 1 | 17 |
| `phase_04_aliases` | Compile-time aliases, canonical targets, cycles | 1 | 23 |
| `phase_05_types` | Names in types, constructors, separate member namespaces | 0 | 27 |
| `phase_06_traversal` | Expression/statement children, switch payloads, label tables | 14 | 79 |

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
File declarations remain predeclared for forward references; compile-time aliases
retain forward-chain resolution. Nested redeclarations of enclosing function
parameters/locals are rejected; sibling scopes may reuse spellings. Initialization
validity, typing, and source-label/generic syntax are outside this specification.

## Local verification (2026-10-19)

`cargo test --offline --no-default-features --no-run` compiles the full suite.
The full library run reports **212 passed, 183 failed**: the 217 new specification
tests contribute 34 passes and 183 expected failures against the unfinished
resolver. These are ordinary failing assertions or unhandled stub panics, so the
full test command exits unsuccessfully; “expected” describes the implementation
status, not a passing test-harness expectation. No cases are ignored. Each phase was also run independently as above.
The previous analyzer baseline had two stale failing stub contracts; those were
replaced with specifications of implemented entrypoints and retained unfinished
AST/typing contracts.

To verify the existing suite independently:

```sh
cargo test --offline --no-default-features -- --skip frontend::analyzer::test::name_resolution
```

That run passes all **178 existing library tests**, with zero binary tests and
zero doctests. Compilation retains 40 pre-existing unused-variable warnings in
unfinished analyzer branches. Formatting and whitespace checks pass. LLVM/default
feature checks and full clippy were not run for this tests-only addition; these
results do not establish a green CI run or an implemented resolver.

## Change guidance

Use `Parser::with_source(Tokenizer::new(source), Path::new("test.fib"), source)`
for disk-free parser tests. Add accepted and malformed syntax cases. For diagnostics,
prefer `is_err()` plus stable category/message/location assertions over full strings.
The name-resolution specification deliberately adds positive and negative
behavioral tests before the pass is complete. Keep explicit-error contracts for
unrelated unfinished passes. Do not return empty success to silence tests. Once
lowering works, add sample reproducers plus verified IR/LLVM and link/run coverage. Avoid chasing 100% coverage with tautological lowering tests.
