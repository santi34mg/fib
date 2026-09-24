# `.github/workflows/ci.yml`

## Current Jobs

- formatting;
- frontend check/test without default features;
- LLVM 21 check, clippy, build, default tests, and all-feature tests;
- advisory non-blocking security audit;
- advisory non-blocking dependency/license checks;
- advisory non-blocking coverage.

The blocking command order matches `AGENTS.md`. Documentation-only changes are
excluded by path filters.

## Current Strengths

- Rust and LLVM versions are explicit;
- frontend-only feature configuration is tested;
- clippy warnings fail the build;
- GitHub token permissions are read-only;
- sample e2e tests run with LLVM in isolated output directories.

## Known Gaps

- default and all-features tests are currently equivalent because LLVM is the
  sole default feature;
- formatting is duplicated across jobs;
- audit/deny/coverage do not gate or enforce a baseline;
- installed cargo tools and some actions are not pinned to immutable revisions;
- CI covers only Ubuntu despite platform-sensitive std/FFI claims;
- there is no release-mode, sanitizer, fuzz, docs-link, rustdoc, std-sweep, or
  deterministic-output job;
- jobs have no explicit timeouts or workflow concurrency policy;
- sample enumeration allows an untested sample to be added silently.

## Improvement Direction

Prioritize correctness gates over raw suite duplication: LLVM verification,
automatic samples, std frontend sweep, real module link/run tests, release
bounds behavior, and parser fuzz smoke tests. Pin advisory tooling and actions,
publish useful reports, and add platform jobs only after platform support is
defined.
