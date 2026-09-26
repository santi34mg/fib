# `tests/`

## Current Suites

- Most unit tests live beside implementation modules under `src/`.
- `tests/e2e.rs` is gated by the LLVM feature. It compiles selected samples into
  fresh temporary directories, runs deterministic binaries, and compares exact
  stdout. `fib_bench` is compile-only.
- `tests/probe_tmp.rs` prints exploratory parser/analyzer/lexer outcomes but
  makes no assertions.

The no-default-features suite gives broad frontend coverage. LLVM-enabled tests
exercise backend unit cases and the manually enumerated sample set.

## Coverage Strengths

- lexer token and malformed-literal cases;
- parser precedence and many syntax shapes;
- analyzer coercion, calls, assignments, enums, slices, imports, and generics;
- driver import order, cycles, canonical aliases, and output precedence;
- direct LLVM aggregate/pointer/sample behavior;
- runtime dynamic index bounds failure.

## Important Gaps

- sample discovery is manual despite documentation saying every sample is tested;
- expected sample output is duplicated in source comments and Rust strings;
- imported user-defined functions are not linked and executed;
- flat IR and direct lowering cannot be forced for differential tests;
- generated LLVM is not verified;
- no subprocess CLI contract tests;
- no standard-library sweep beyond sample use of `std::libc`;
- no fuzzing/property tests;
- no determinism tests;
- no runtime coverage for dynamic slice failures or release bounds behavior;
- print-only probes cannot detect diagnostic regressions.

## Test Policy for Changes

- New syntax: lexer/parser positive and malformed-input tests.
- New semantic rule: analyzer success and negative tests with stable substrings.
- New lowering branch: a runnable sample when practical.
- Soundness fix: a negative analyzer test and a backend regression if the bug
  previously reached lowering.
- Module change: filesystem fixture plus link-and-run coverage.
- Shared lowering feature: force both routes, verify LLVM, and compare execution.
- Diagnostic change: assert category, message substring, and source range rather
  than the complete rendered message.
