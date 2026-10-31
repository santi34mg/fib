# Configured CI versus current implementation

Source: [`.github/workflows/ci.yml`](../../../../.github/workflows/ci.yml).
CI pins Rust 1.98.0, LLVM 21 via `ZhongRuoyu/setup-llvm@v0`, Ubuntu runners,
and read-only contents permission.

- `fmt`: `cargo fmt --check`.
- `frontend`: `cargo check --no-default-features`, then
  `cargo test --no-default-features`.
- `build_and_test`: fmt, check, `cargo clippy --all-targets -- -D warnings`,
  build, `cargo test --all-features` in that order after LLVM setup.
- `audit`, `deny`, `coverage`: advisory `continue-on-error: true`, running
  cargo-audit, cargo-deny, cargo-tarpaulin respectively.

Jobs are separate, not one sequential global gate. Default and all-features
currently select the same sole `llvm` feature, so only the all-features LLVM
test command runs. Frontend and LLVM jobs cache Cargo artifacts under separate keys.
The frontend job runs unit and integration tests without LLVM; native executable
and sample acceptance tests run in the LLVM job. Cache configuration does not
change the selected tests.

Push/PR workflows target `main` and ignore `docs/**`, `*.md`, `LICENSE`, and
`.gitignore`; docs-only updates do not run these gates. The optional local
pre-commit hook also runs blocking cargo-deny, unlike CI's advisory deny.

Configuration is not a green-build claim. See [local results](../../tests/README.md).
Remaining useful future gates include docs links, fuzzing and determinism.
No release-mode or cross-platform coverage should be inferred from these jobs.
