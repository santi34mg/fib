# Testing Strategy and Gaps

> Note (2026-09-18): addressed items removed. This file now lists only
> partially addressed points. Removed: e2e sample harness (`tests/e2e.rs`
> compiles + runs all 13 samples with stdout asserts, wired into CI).

## Current state (verified 2026-09-18)

- `tests/e2e.rs:1-167` (`#[cfg(feature = "llvm")]`) iterates `samples/*.fib` via `compile_project`, executes, asserts exit code + exact stdout; `fib_bench` is compile-only; runs inside `build_and_test` via `cargo test`.
- `src/backend/lowering/test.rs` (8 tests, gated `#[cfg(all(test, feature = "llvm"))]`, `Context::create()` per test): `coerce_int_to_llvm_type` happy path (`:34-73`), `map_type_to_llvm` unsigned widths + `Void`-is-err (`:75-100`), `UnknownLayout` (`:140-164`), one `compute_lvalue_ptr` error (`:166-191`), `lower_ir` incl. `udiv/ugt` (`:102-138`).
- Parser tests: `get_ast` still panics on `Err` (`parser/test.rs:14-35`); `get_ast_err` added (`:652-670`) with 7 negatives (`:672,678,688,698,704,714,852`); precedence shapes covered positively (`:722-810`), `QualifiedAccess` vs `BuiltinCall` (`:814-849`).
- Analyze tests: 23 `get_typed_err` call sites with substring/line asserts; dup/missing struct field (`:770,778`), arity (`:786`), switch non-enum/payload (`:793,799`), escapes (`:824,830`), selective/unknown imports (`:909,920`) covered. But `test_return_type_mismatch_errors` (`:930-934`) and `test_break_outside_loop_errors` (`:936-940`) are `#[ignore]`d known-gaps; no mutability-violation or switch-exhaustiveness negatives.
- Lexer matrices done (`test_all_keywords:150-177` all 21 variants, `test_operators:179-218` incl. `%=,->,.., ...`, `test_punctuation:220-241` incl. `.,::,@`); error-branch coverage is only unterminated string as `Error` token (`:541`) + via parser (`parser/test.rs:672`), `@nope` (`:434,544`), `$` only indirectly via parser (`:688-696`).
- Policy prose exists (`CONTRIBUTING:34-39` + PR-template checklist); no `tarpaulin`/`llvm-cov` job anywhere.

## Remaining points

### 1. Backend unit tests without LLVM sweat [PARTIALLY ADDRESSED]
Covered: happy-path `coerce_*`, unsigned/`Void` mapping, one `compute_lvalue_ptr` error, `lower_ir` smoke.

Remaining: `coerce_*` error paths (currently only `Ok` asserted), more `compute_lvalue_ptr` error shapes. Keep gating backend tests behind `#[cfg(feature = "llvm")]` and frontend tests feature-free.

### 2. Negative parser/analyze tests [PARTIALLY ADDRESSED]
Remaining:
- Parser: change `get_ast` to return `Result` (still panics), grow from 7 to ~15 negatives. Missing: more EOF shapes, `break`-outside-loop at parse level if applicable, duplicate struct fields, `break` outside loop / return-mismatch at analyze level (see below).
- Analyze: un-`#[ignore]` and enforce `return-type-mismatch` (`:930-934`) and `break-outside-loop` (`:936-940`) — the checks don't exist yet, the tests lock nothing. Add mutability-violation and switch-exhaustiveness negatives.
- Assert style stays: `is_err()` + substring/line, not exact full message, until diagnostics stabilize.

### 3. Lexer edge cases [PARTIALLY ADDRESSED]
Matrices done. Remaining: unterminated char, char-escape/hex-escape/string-escape error branches (`lexer.rs:314,325,380,385,399`), direct `TokenKind::Unknown` test (`lexer.rs:82`; `$` is only covered indirectly via the parser). Char escapes only have positive tests (`:330,337`).

### 4. Coverage policy [PARTIALLY ADDRESSED]
Policy half done (reproducer-per-branch + `get_typed_err`-per-fix, enforced by PR template). Remaining: add `cargo tarpaulin` or `llvm-cov` as advisory (not gating) in CI. Do NOT chase 100% line coverage on the lowering files — it incentivizes tautological tests.

Cost ordering: lexer edges (hours) < negatives (days) < backend error paths (days) < coverage tuning. Lexer + negatives should precede any parser-dedup sequel; e2e net already exists for IR work.
