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
- `frontend/analyzer/test.rs`: working lexical scopes/identity utilities, pending
  annotations, and failure/no mutation from unimplemented passes, not type checking.
- `diagnostics.rs`: parser conversion and rendering/line extraction.
- `driver/test.rs`: pipeline and lowering stubs explicitly fail.
- `ir/test.rs`: raw representation/access/display contracts and unfinished API failure.
- `backend/lowering/test.rs`: backend sketch boundaries do not fabricate success.

## Local verification (2026-10-17)

`cargo check --offline --no-default-features` succeeded.
`cargo test --offline --no-default-features` passed 173 library tests, including
sample parser discovery; binary tests
and doctests each reported zero. `cargo run --offline --no-default-features -- --help`
succeeded. System rustc/Cargo report 1.98.0, with rustup unavailable.

`cargo fmt --check` passed (Cargo exit status 0), and `git diff --check`
reported no whitespace errors.
Default-feature LLVM checks, full clippy/build/tests and all-features were not run
for this sample migration. Do not infer that configured CI is green.

## Change guidance

Use `Parser::with_source(Tokenizer::new(source), Path::new("test.fib"), source)`
for disk-free parser tests. Add accepted and malformed syntax cases. For diagnostics,
prefer `is_err()` plus stable category/message/location assertions over full strings.
For future semantic rules add real positive and negative tests as passes execute;
keep explicit-error contracts until then. Do not return empty success to silence
stub tests. Once lowering works, add sample reproducers plus verified IR/LLVM and
link/run coverage. Avoid chasing 100% coverage with tautological lowering tests.
