# Repository map and commands

The [language specification](../language-specification.md) is implemented for prelude names, casts, shallow readonly views,
warned pointer field access and exhaustive switches. Concrete generic functions,
multiple source files and fixed-array values execute. Package metadata/scoped
visibility exist; package-rooted lookup/discovery and runtime indexing/slices remain deferred.

## Cargo and toolchain

[`Cargo.toml`](../../Cargo.toml) defines one Rust 2024 package, `fibc` 0.1.1,
with library and binary. Default feature `llvm` enables optional Inkwell 0.8
(`llvm21-1`); default dependency builds therefore require LLVM 21. The backend
emits scalar/struct/enum/pointer LLVM objects. No-default-features builds avoid LLVM.
Dependencies also include anyhow, serde, toml, clap, regex, walkdir, petgraph,
either, indextree, and tempfile; do not remove them based on old usage inventories.

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
tests and helpers live in `src/frontend/analyzer/test.rs`, with six inline
`name_resolution` phases. These filters select Rust unit tests, not Fib program
compilation.

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
