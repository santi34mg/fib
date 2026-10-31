# Rust build and test performance

## Workspace split

The compiler now uses [stage-specific workspace crates](../crates/README.md).
Root Cargo commands include every package by default; package-specific tests
can avoid compiling unrelated stages. The frontend and IR depend only on shared
builtin identities, lowering depends on both, and backend depends only on IR.
The `fibc` facade owns the driver and binaries. Cross-stage tests remain in the
facade's integration suite.

A warm rebuild comparison on 2026-10-30 changed the backend invalid-error prefix
from `backend:` to `backend error:` in both the monolithic and split compiler:

| LLVM test compilation after the edit | Monolithic | Split workspace |
| --- | ---: | ---: |
| Complete test build, without execution | 2.15 s | 1.47 s |

This is one sequential local comparison (about 32% lower elapsed time), not a
general speedup guarantee. Both trees used the preceding dependency removals,
line-table debug settings and empty-harness removal, with dependencies and
incremental artifacts already built. Baseline sources came from `2163821` with
those Cargo adjustments reapplied. The commands were `cargo test --offline
--no-run --timings` on the baseline and `cargo test --offline --workspace
--all-features --no-run --timings` on the workspace, with LLVM 21 configured.
The temporary diagnostic edit was reverted afterwards. No other build/test jobs
ran during these two measurements. This does not measure cold builds or test
execution time.

Cargo's workspace timing report confirms the backend and facade rebuilt while
frontend (normal and unit-test builds), IR and lowering were all cached. The
reports are in `target/crate-split-before-build/cargo-timings/` and
`target/cargo-timings/`. Frontend-only work can use `cargo test -p fib-frontend`
without LLVM or later compiler stages. Both complete configurations retain the
same test totals: 870 with LLVM, 798 without. Strict all-target Clippy passed
in both configurations.

## Earlier Cargo profile and dependency changes

These changes target Cargo's compilation of the Rust compiler, rather than the
speed of compiling Fib programs. Development/test builds use line-table debug
info, remove four unused direct dependencies (and their unused transitive
dependencies), and skip the two empty compiler/installer unit-test harnesses.
The LSP's real unit tests and all integration tests remain enabled.

CI and the local pre-commit hook run the LLVM suite once with all features;
default and all features currently select the same `llvm` feature. The separate
no-default-features suite remains. The frontend CI job now caches Cargo artifacts
and dependencies, using a key separate from the LLVM job.

## Measurement on 2026-10-30

Using baseline commit `2163821`, Rust 1.98.0 and LLVM 21 on the same local
machine, sequential builds into fresh target directories produced:

| LLVM-enabled test build | Before | After |
| --- | ---: | ---: |
| `cargo test --offline --no-run --timings` | 14.79 s | 11.90 s |
| Target directory size (`du -sh`) | 2.1 GiB | 1.6 GiB |

This is a single warm-filesystem-cache comparison, including dependency builds,
with no concurrent benchmark/test jobs. It suggests about a 20% reduction in
fresh test compilation time; it does not establish an incremental-build or
test-execution speedup. The compiler implementation and optimization levels
are unchanged. Removing the duplicate suite saves one complete warm test run
in CI and the hook, rather than reducing individual test execution times.

Reproduce on the baseline and updated checkout respectively with distinct,
unused target directories:

```sh
CARGO_TARGET_DIR=target/build-time-baseline LLVM_SYS_211_PREFIX=/usr/lib/llvm/21 cargo test --offline --no-run --timings
CARGO_TARGET_DIR=target/build-time-after LLVM_SYS_211_PREFIX=/usr/lib/llvm/21 cargo test --offline --no-run --timings
```

Cargo writes per-target timing reports beneath each target directory's
`cargo-timings/`. Use the local LLVM installation path where different.
For full debugger variable inspection, override line-table info with
`CARGO_PROFILE_DEV_DEBUG=2` or `CARGO_PROFILE_TEST_DEBUG=2`. Source file/line
backtraces remain available by default. Release/bench profiles are unchanged.

Validation passed: 870 LLVM-enabled tests, 798 no-default-features tests,
strict LLVM Clippy for all targets, formatting and whitespace checks. Feature
configurations were validated separately to avoid overwriting shared CLI binaries
while integration tests were running.
