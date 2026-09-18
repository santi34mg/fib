# Testing Strategy and Gaps

## Current state (verified)

- 123 `#[test]` total, all in-tree unit tests: `lexer/test.rs:448L` (36 tests), `parser/test.rs:649L` (39 tests + `get_ast:14`, `module_statements:34`), `analyze/test.rs:620L` (48 tests + `get_typed:14`, `get_typed_err:23`, `get_function:34`).
- No `tests/` integration directory. `cargo test` in `ci.yml:54-55` only runs unit tests.
- Zero tests for `llvm_lower.rs` (2050 lines), `ir/mod.rs`, `driver.rs`, `cli.rs`, `typed_ast.rs` (`SymbolTable` push/pop/leak, shadowing, `Display for Ty`, `then/else_branch_terminates`).
- Lexer gaps: `test_all_keywords:150` misses `type/enum/union/fn/switch/when`; `test_operators:173` misses `%=,->,.., ...`; `test_punctuation:210` misses `.,::,@`; error tests only `0b4,0o9,0xGG,@nope` — no unterminated `"/'/\\` despite `lexer.rs:270,281,348,353,367`.
- Parser gaps: no coverage for `switch`, `struct_literal`, `enum_literal`, `tuple_type`, `import`, `type_decl/function_type`, `QualifiedAccess`, `BuiltinCall`, shift/bitwise/logical_or precedence, `union/enum/struct` defs. `get_ast:30` panics on `Err`, so zero negative tests.
- Analyze gaps: happy-path heavy; only 5 `get_typed_err` (`:485,494,552,610,616`); missing return-mismatch, arity, mutability, `break` outside loop, struct/enum/switch/tuple/import errors, generics instantiation (`instantiate_generic:1907`), selective vs aliased imports (`:65-100`), `process_escape_sequences:2017` error branches.
- `samples/`: 13 working `.fib` files (`hello_world`, `minimal`, `enums`, `switch_enum`, `tagged_union`, `sorting`, `linked_list`, `memory_pool`, `fib_bench`, etc.) — an untapped e2e corpus. `out/hello_world[.ll]` are local build artifacts (ignored).

## Opportunities

### 1. E2E sample harness (highest leverage)
Add `tests/e2e.rs` that iterates `samples/*.fib`, compiles with `compile_project` (or `cargo run`), executes the binary, asserts exit code/stdout. Start with the 3-5 deterministic samples (`hello_world`, `minimal`, `enums`, `switch_enum`); mark `fib_bench`, `sorting` as smoke-only (compile + run, no timing assert).

Trade-offs:
- **Pro:** catches cross-stage regressions (analyze + lowering + clang flags) that unit tests cannot. Turns `samples/` into a spec.
- **Con:** slow (LLVM + clang per case), platform-dependent (needs `clang-17|clang`), flaky if samples print pointers/timings. Mitigate: `#[ignore]` for bench, cache `target/`, run e2e only on `main` CI job, unit tests on every PR.
- Alternative: `tests/snapshots/` asserting generated `.ll` text. Faster, no clang needed, but brittle to inkwell version churn (LLVM 21 strings change often). Prefer execution asserts over IR snapshots, except for one or two tricky cases (unsigned div, short-circuit).

### 2. Backend unit tests without LLVM sweat
Even before e2e, test pure helpers: `coerce_int_to_llvm_type:295`, `map_type_to_llvm:1216` for every `Ty` (especially unsigned + `Void`), `compute_lvalue_ptr:225` error paths. Use `Context::create()` per test — cheap.

Trade-off: inkwell tests require LLVM libs in CI (already present via `ZhongRuoyu/setup-llvm`), but `cargo test --no-default-features` contributors lose coverage. Gate backend tests behind `#[cfg(feature="llvm")]` and keep frontend tests feature-free (see file 03 option A).

### 3. Negative parser/analyze tests
Change `parser/test.rs:get_ast` to return `Result` and add `get_ast_err`; add ~15 cases: unexpected EOF in import/switch, bad precedence (`a || b && c` tree shape), `QualifiedAccess` vs `BuiltinCall`, duplicate struct fields, `break` outside loop, return-type mismatch, mutability violation, switch non-exhaustive/payload mismatch.

Trade-offs:
- **Pro:** locks in diagnostics before refactoring precedence parsers (file 04).
- **Con:** over-pinning error strings makes refactoring painful. Assert on `is_err()` + substring/line, not exact full message, until diagnostics stabilize.

### 4. Lexer edge cases (quick wins)
Add unterminated string/char/escape tests, `Unknown` char test, full keyword/operator/punctuation matrices. These are <1h work and prevent regressions when splitting `lex_token`.

### 5. Coverage policy
Do NOT chase 100% line coverage on a 2000-line lowering file — it incentivizes tautological tests. Instead: require (a) every new lowering branch ships with a `samples/*.fib` reproducer, (b) every soundness fix in file 02 ships with a `get_typed_err` test. Add `cargo tarpaulin` or `llvm-cov` as advisory (not gating) in CI.

Cost ordering: (4) hours < (3) days < (2) days < (1) 1-2 days + CI tuning. Do (4)+(3) before the parser dedup; do (1) before the IR migration so you have a regression net.
