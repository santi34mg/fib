# Project Health, CI, and Developer Experience

> Note (2026-09-18): addressed items removed. This file now lists only
> partially addressed points. Removed: branch/CI alignment (trunk-based
> `main`, `pull_request` triggers, `permissions: read`, docs-only path
> filter), docs/onboarding (badge, `--help`, `-I`, clang troubleshooting,
> real `samples`/`std` READMEs, sample output oracles), hygiene (fresh lock,
> `out/` ignored, no stray configs, Keep-a-Changelog, PR template with
> conventional titles).

## Current state (verified 2026-09-18)

- `ci.yml`: `push` + `pull_request` on `main`, `permissions: contents: read`, docs-only path filter, standalone `fmt` job (also `Format`-before-`Check` fail-fast in `build_and_test:91-95`), `frontend` job for `--no-default-features` (`:37-55`), `Test` + `Test (all features)` (`:103-107`), advisory non-blocking `cargo audit` (`:109-124`, `continue-on-error: true`).
- No `rust-toolchain*` file; toolchain is unpinned `dtolnay/rust-toolchain@stable`. Cache key hashes `Cargo.lock` + `ci.yml` (`:86`) with a comment claiming toolchain bumps invalidate it — they don't reliably.
- No `cargo deny` config. No tags/releases; version still `0.0.1` (`Cargo.toml:3`). MSRV is implicit only (`README.md:19` "stable", `ci.yml:44-46` comment about pinning a date if stable breaks).

## Remaining points

### 1. Harden CI incrementally [PARTIALLY ADDRESSED]
Done: (a) `--all-features` + `--no-default-features` jobs, (b) early `fmt --check`, (d-half) advisory non-blocking `cargo audit`.

Remaining:
- (c) Real toolchain-aware cache key: pin with `rust-toolchain.toml` (or record `rustc --version` into the key) instead of relying on `Cargo.lock` + `ci.yml` hashing while tracking floating `stable`.
- (d) `cargo deny` (advisory, non-blocking like `audit`). Keep both non-gating — inkwell/LLVM advisories would otherwise block unrelated PRs.

Trade-offs: multi-OS matrix (ubuntu/macos/windows) and MSRV sound nice but LLVM 21 setup triples CI minutes and flakes on Windows. Defer until stdlib stabilizes; ubuntu-only is the right call now.

### 2. Versioning and release story [PARTIALLY ADDRESSED]
Still `0.0.1`, `git tag` empty — no tags/releases. MSRV is policy-comment only, no number or pinned date.

Remaining: decide nightly `out/` binaries vs. `cargo install --git` vs. GitHub Releases with prebuilt `fibc`. Until then, pin the toolchain date that last went green (per the `ci.yml:44-46` comment's own advice) so MSRV is reproducible, not just "stable".

Suggested order: toolchain pin + cache key (30 min) -> `cargo deny` advisory (30 min) -> first tag/release decision.
