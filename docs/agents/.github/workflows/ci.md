# Configured CI versus current implementation

Source: [`.github/workflows/ci.yml`](../../../../.github/workflows/ci.yml).
CI pins Rust 1.98.0, LLVM 21 via `ZhongRuoyu/setup-llvm@v0`, Ubuntu runners,
and read-only contents permission.

- `fmt`: `cargo fmt --check`.
- `frontend`: `cargo check --no-default-features`, then
  `cargo test --no-default-features`.
- `build_and_test`: fmt, check, `cargo clippy --all-targets -- -D warnings`,
  build, test, `cargo test --all-features` in that order after LLVM setup.
- `audit`, `deny`, `coverage`: advisory `continue-on-error: true`, running
  cargo-audit, cargo-deny, cargo-tarpaulin respectively.

Jobs are separate, not one sequential global gate. Default and all-features
currently select the same sole `llvm` feature. LLVM dependency installation does
not make the sketched backend an emitter. The frontend job's comment mentioning
`tests/e2e.rs` is stale: no root integration/e2e suite exists. CI executes current
Rust unit/contract tests, not sample binaries.

Push/PR workflows target `main` and ignore `docs/**`, `*.md`, `LICENSE`, and
`.gitignore`; docs-only updates do not run these gates. The optional local
pre-commit hook also runs blocking cargo-deny, unlike CI's advisory deny.

Configuration is not a green-build claim. See [local results](../../tests/README.md).
Remaining useful future gates include docs links, actual sample execution once
compilation works, std checks, verifier regressions, fuzzing and determinism.
No release-mode/sample/runtime/platform coverage should be inferred today.
