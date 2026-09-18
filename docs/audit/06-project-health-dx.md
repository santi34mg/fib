# Project Health, CI, and Developer Experience

## Current state (verified)

- `CONTRIBUTING:1-14`: PRs to `dev`, issues/discussions welcome, “follow formatter/linters” with no commands/names.
- `.github/workflows/ci.yml:1-55` (`Dev CI`): LLVM 21 + `cargo check`, `fmt --check`, `clippy -- -D warnings`, `build`, `test`. Triggers only on `main` (`:6-10`), mismatching `CONTRIBUTING` (`dev`). `permissions: contents: write (:3-4)` is overbroad for a check workflow.
- `.gitignore` healthy (`target/`, `out/`, `*.ll`, `*.rs.bk`, `*.pdb`). `git ls-files out/` empty — `out/hello_world[.ll]` are local-only, correct.
- `Cargo.toml:1-19` (`fibc 0.0.1`, edition 2024, default `llvm = [inkwell llvm21-1]`). `Cargo.lock` dated 2026-07-08, HEAD 2026-09-17 with 4 Sept commits after lock — run `cargo update/check`.
- `docs/`: 16 files (`README` index + 15 topics), good. `samples/`: 13 `.fib` + stub `README.md` (“temporary directory…”). `std/`: `core/ fs/ io/ serialization/ libc.fib` + stub `README.md`.
- No badges, no `CHANGELOG`, no `rustfmt.toml`/`clippy.toml` (defaults enforced by CI, fine).
- `git log --oneline`: HEAD `1a68e3e IR implementation, AI assisted (2026-09-17)` on `refactor/better-modules`, diverged from `origin/main` (`4973c37` from 07-05). Messages uneven (`idk`, `beggining` typo). `git status`: clean.

## Opportunities

### 1. Align branches + CI triggers
Either (A) change `CONTRIBUTING` to PR-to-`main` (simplest, matches CI), or (B) add `dev` to `ci.yml:7-10` and make `main` a release-only gate.

Trade-offs:
- **(A) trunk-based:** less process, suits <5 contributors. Risk: `main` breaks more often.
- **(B) `dev` + `main`:** safer releases, but stale long-lived `refactor/better-modules` (already diverged ~2 months) shows the cost — require rebasing weekly or the branch rots.
- Regardless: fix `permissions: contents: read` (write is unnecessary and a supply-chain smell), and add `pull_request` path filtering so docs-only edits skip LLVM setup.

### 2. Harden CI incrementally
Current pipeline is solid for a 0.0.1. Next, in order:
1. `cargo test --all-features` + `cargo test --no-default-features` (catches file-03 silent-noop gating).
2. `cargo fmt --check` before `check` (fail fast, cheaper).
3. Cache key includes `rust-toolchain` version, not just `Cargo.lock`.
4. Advisory `cargo audit` / `cargo deny` (do NOT gate on RUSTSEC on day one — inkwell/LLVM advisories will block unrelated PRs).

Trade-offs: multi-OS matrix (ubuntu/macos/windows) and MSRV sound nice but LLVM 21 setup triples CI minutes and flakes on Windows. Defer until stdlib stabilizes; ubuntu-only is the right call now.

### 3. Docs and onboarding
- `README.md:29-36` hello-world works, but no CI badge, no `--help` output, no `-I` explanation, no troubleshooting for missing `clang-17`.
- `samples/README.md` + `std/README.md` stubs actively harm discovery. One paragraph each + per-file one-liners (`sorting.fib: bubble sort over @int[]`) is enough.
- `docs/README.md:23` links samples — add expected-output comments inside each `samples/*.fib` so the e2e harness (file 05) can reuse them as oracles.

Trade-off: docs rot. Keep docs executable where possible (doc-tests, e2e asserts) rather than prose mirrors of `docs/types.md`.

### 4. Hygiene (30-minute wins)
- `cargo fmt && cargo update && git add Cargo.lock`; delete local `out/` artifacts or leave ignored (do NOT commit).
- Add `rustfmt.toml` only if you need non-defaults — otherwise it is noise. Same for `clippy.toml`.
- Start `CHANGELOG.md` (Keep-a-Changelog, `Unreleased` section) now; backfill Sept IR refactor + dropped self-hosted effort (`8d62e11`) while memory is fresh. Cost is 5 min/PR, payoff is release notes for free.
- Squash-merge with conventional titles (`feat(frontend): ...`, `fix(backend): ...`); forbid `idk`-style messages via a PR template, not a bot.

### 5. Versioning and release story
`0.0.1` + `edition 2024` is fine, but no tags/releases in repo. Decide: nightly `out/` binaries vs. `cargo install --git` vs. GitHub Releases with prebuilt `fibc`. Until then, document MSRV implicitly via `dtolnay/rust-toolchain@stable` pin date.

Suggested order: permissions fix + branch alignment (10 min) -> CHANGELOG + sample/std READMEs (1h) -> `--no-default-features` CI job (30 min) -> e2e harness (file 05).
