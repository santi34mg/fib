# Project Health, CI, and Developer Experience

> Note (2026-09-18): addressed items removed. This file now lists only the
> release/versioning decision. Removed: branch/CI alignment (trunk-based
> `main`, `pull_request` triggers, `permissions: read`, docs-only path
> filter), docs/onboarding (badge, `--help`, `-I`, clang troubleshooting,
> real `samples`/`std` READMEs, sample output oracles), hygiene (fresh
> lock, `out/` ignored, no stray configs, Keep-a-Changelog, PR template
> with conventional titles), §1 CI hardening items (c) and (d),
> the §2 release/versioning decision (see below).

## Current state (verified 2026-09-18)

- Toolchain pinned: `rust-toolchain.toml` pins `1.98.0`; the cache key in
  `ci.yml` hashes `Cargo.lock` + `ci.yml` + `rust-toolchain.toml` so a
  toolchain bump invalidates the cache.
- `ci.yml` jobs: `fmt`; `frontend` (`--no-default-features`); `build_and_test`
  (fmt-before-check fail-fast, `cargo check`, `clippy -D warnings`,
  `cargo test`, `cargo test --all-features`); three advisory
  non-blocking jobs — `audit`, `deny` (cargo-deny check), `coverage`
  (tarpaulin) — none of which gate merges.
- Version `0.1.0` (`Cargo.toml:3`) with **`v0.1.0` tagged on `main`**
  (trunk-based: feature branch `refactor/better-modules` fast-forwarded;
  main stays at the release commit until the next change). Install is
  build-from-source (no prebuilt binaries), matching the deferred multi-OS
  matrix: LLVM 21 setup triples CI minutes and flakes on Windows, and
  shipping prebuilt LLVM-linked binaries would raise MIT/Dual licensing
  questions. GitHub Release `v0.1.0` carries the build instructions; the
  CHANGELOG links `[0.1.0]` to it.

## Remaining points

- None. (Future observations marked here until triaged.)