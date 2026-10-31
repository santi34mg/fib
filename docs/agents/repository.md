# Repository map and commands

The [language specification](../language-specification.md) is implemented for prelude names, casts, shallow readonly views,
warned pointer field access and exhaustive switches. Concrete generic functions,
multiple source files and fixed-array values execute. Package-rooted lookup, scoped
visibility, installed std sources and Linux x86-64 syscalls execute. Recursive source
discovery remains deferred. Checked runtime indexing/slicing and constant-initialized
globals execute.

## Cargo and toolchain

[`Cargo.toml`](../../Cargo.toml) defines a Rust 2024 workspace with `fibc` 0.2.0
as its facade, CLI and driver package. Internal packages separate builtin
identities, frontend analysis, owned SSA, lowering and LLVM emission; see the
[crate map](../../crates/README.md). Default members include every package, so
root check/test/Clippy commands cover their unit tests. Default feature `llvm` enables optional Inkwell 0.8
(`llvm21-1`); default dependency builds therefore require LLVM 21. The backend
emits scalar/struct/enum/pointer LLVM objects. No-default-features builds avoid LLVM.
Dependencies also include anyhow, num-bigint, num-traits, rustc_apfloat, serde,
serde_json, toml, clap, walkdir, and tempfile. Unused direct dependencies on regex,
petgraph, either and indextree were removed after checking current Rust sources.

Development and test builds use line-table debug info to reduce code generation
and linking costs while retaining source locations in backtraces. For debugger
variable inspection, use `CARGO_PROFILE_DEV_DEBUG=2` (or
`CARGO_PROFILE_TEST_DEBUG=2` for tests). Release and benchmark profiles are unchanged.
The compiler and installer binaries have no unit tests and skip empty test
harnesses; Cargo still builds them for integration tests. The LSP unit tests remain enabled.
The `fib-dist` Rust tool builds a relocatable Linux x86-64 archive via the installer,
tests it after extraction, and emits a SHA-256 checksum. Release publication uploads
that bundle; packaging PRs/manual runs produce Actions artifacts. See the
[release handoff](../releases/0.2.0.md#release-handoff) for commands and requirements.
See [Rust build measurements](../rust-build-performance.md) for scope and reproduction.

[`rust-toolchain.toml`](../../rust-toolchain.toml) pins 1.98.0 with rustfmt/clippy;
its comments define the pin as MSRV policy. Bump deliberately with CI, not by drift.

## Local validation

Local validation uses cached offline dependencies:

```sh
cargo check --offline --no-default-features
cargo test --offline --no-default-features
cargo fmt --check
cargo clippy --offline --no-default-features --all-targets -- -D warnings
```

The unfiltered no-default-features suite passes, including all six former pending
acceptance cases. See
[test commands and measured results](tests/README.md). Offline mode requires a
populated cache and is optional outside restricted environments. The CLI builds
the supported multi-file scalar/aggregate/control-flow subset
with LLVM and host `cc`, either directly or through a narrow interpreted build
file. Default discovery, `run` and `test` remain unfinished.

Useful focused commands: `cargo test --no-default-features frontend::parser`,
`cargo test --no-default-features frontend::tokenizer`, and
`cargo test --no-default-features name_resolution::phase_01_files`. All analyzer
tests and helpers live in `crates/fib-frontend/src/frontend/analyzer/test.rs`, with six inline
`name_resolution` phases. These filters select Rust unit tests, not Fib program
compilation.

For faster isolated work, use `cargo test -p fib-frontend`, `cargo test -p fib-ir`,
or `cargo test -p fib-backend --features llvm`. Frontend/IR package tests require
no LLVM installation. `cargo test -p fibc` only selects the facade package;
use root `cargo test` or `cargo test --workspace` for the complete suite.

## Assets

- [`src/`](../../src): active compiler modules.
- [`samples/`](../../samples): current-syntax examples with automatic parser coverage
  and selected scalar/loop/struct/enum execution coverage.
- `tests/executable.rs` compiles, links and executes supported scalar/aggregate/control-flow programs with LLVM.
- [`CI`](.github/workflows/ci.md) records configured gates, not proof they pass.
- [`.githooks/pre-commit`](../../.githooks/pre-commit) is opt-in; it skips docs-only
  staged changes and runs `cargo deny check` as well as frontend/full gates.
  Unlike advisory CI deny, local deny failure blocks the hook.

The project model uses explicitly named files in `BuildPlan`. The explicit
`--build-file` route compiles a `build() int32` entry to verified owned IR and
interprets literal `build_api` operations against host-owned builder state.
Finalization validates deterministic source identities and root-relative paths,
names, entry and output. General interpretation and default discovery remain
unfinished. Build files do not emit or load native build-file artifacts, and
source membership is explicit rather than recursively discovered through imports. See the
[future build-file plan](../roadmap/08-interpreted-build-files.md).
