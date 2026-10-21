# Repository map and commands

## Cargo and toolchain

[`Cargo.toml`](../../Cargo.toml) defines one Rust 2024 package, `fibc` 0.1.1,
with library and binary. Default feature `llvm` enables optional Inkwell 0.8
(`llvm21-1`); default dependency builds therefore require LLVM 21 even though
the initial scalar backend emits LLVM objects. No-default-features builds avoid LLVM.
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
populated cache and is optional outside restricted environments. The CLI builds the single-file scalar/conditional subset with LLVM and host `cc`;
other commands remain unfinished.

Useful focused commands: `cargo test --no-default-features frontend::parser`,
`cargo test --no-default-features frontend::tokenizer`, and
`cargo test --no-default-features name_resolution::phase_01_files`. All analyzer
tests and helpers live in `src/frontend/analyzer/test.rs`, with six inline
`name_resolution` phases. These filters select Rust unit tests, not Fib program
compilation.

## Assets

- [`src/`](../../src): active modules plus explicitly uncompiled legacy code.
- [`samples/`](../../samples): current-syntax examples with automatic parser coverage,
  not an executable regression suite.
- [`std/`](../../std): experimental Fib sources, not automatically checked.
- `tests/executable.rs` compiles, links and executes the initial milestone with LLVM.
- [`CI`](.github/workflows/ci.md) records configured gates, not proof they pass.
- [`.githooks/pre-commit`](../../.githooks/pre-commit) is opt-in; it skips docs-only
  staged changes and runs `cargo deny check` as well as frontend/full gates.
  Unlike advisory CI deny, local deny failure blocks the hook.

The planned project model uses explicitly named files in `BuildPlan`, optionally
produced by compiling `build.fib` to verified owned IR and interpreting it through
an opaque host-owned build API. It will not emit or load a native build-file
artifact. This is not the former include-root/recursive import model; build-file
interpretation is not implemented yet. See the
[future build-file plan](../roadmap/08-interpreted-build-files.md).
