# Project Health, CI, and Developer Experience

> Note (2026-09-18): addressed items removed. This file now lists only the
> release/versioning decision. Removed: branch/CI alignment (trunk-based
> `main`, `pull_request` triggers, `permissions: read`, docs-only path
> filter), docs/onboarding (badge, `--help`, `-I`, clang troubleshooting,
> real `samples`/`std` READMEs, sample output oracles), hygiene (fresh
> lock, `out/` ignored, no stray configs, Keep-a-Changelog, PR template
> with conventional titles), §1 CI hardening items (c) and (d).

## Current state (verified 2026-09-18)

- Toolchain pinned: `rust-toolchain.toml` pins `1.98.0`; the cache key in
  `ci.yml` hashes `Cargo.lock` + `ci.yml` + `rust-toolchain.toml` so a
  toolchain bump invalidates the cache.
- `ci.yml` jobs: `fmt`; `frontend` (`--no-default-features`); `build_and_test`
  (fmt-before-check fail-fast, `cargo check`, `clippy -D warnings`,
  `cargo test`, `cargo test --all-features`); three advisory
  non-blocking jobs — `audit`, `deny` (cargo-deny check), `coverage`
  (tarpaulin) — none of which gate merges.
- No tags/releases; version still `0.0.1` (`Cargo.toml:3`). MSRV is now a
  concrete number via the `rust-toolchain.toml` pin, not just "stable".

## Remaining points

### 2. Versioning and release story
No tags or releases exist; install is build-from-source. Remaining:
decide nightly `out/` binaries vs. `cargo install --git` vs. GitHub
Releases with prebuilt `fibc`, and MIT/Dual licensing of binaries if ship
prebuilt LLVM-linked ones.

Trade-offs: multi-OS matrix (ubuntu/macos/windows) is deferred — LLVM 21
setup triples CI minutes and flakes on Windows. This is a product decision
for the maintainer, not an engineering gap; revisit when the stdlib
stabilizes.