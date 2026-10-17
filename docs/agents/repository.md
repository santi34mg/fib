# Repository map and commands

## Cargo and toolchain

[`Cargo.toml`](../../Cargo.toml) defines one Rust 2024 package, `fibc` 0.1.1,
with library and binary. Default feature `llvm` enables optional Inkwell 0.8
(`llvm21-1`); default dependency builds therefore require LLVM 21 even though
active backend code emits nothing. No-default-features builds avoid LLVM.
Dependencies also include anyhow, serde, toml, clap, regex, walkdir, petgraph,
either, indextree, and tempfile; do not remove them based on old usage inventories.

[`rust-toolchain.toml`](../../rust-toolchain.toml) pins 1.98.0 with rustfmt/clippy;
its comments define the pin as MSRV policy. Bump deliberately with CI, not by drift.

## Verified locally (2026-10-17)

System Cargo and rustc report 1.98.0; rustup is unavailable in this environment.
Commands below were run with offline dependencies already present:

```sh
cargo check --offline --no-default-features
cargo test --offline --no-default-features
cargo run --offline --no-default-features -- --help
cargo fmt --check
```

Check succeeded; tests reported 173 passing library tests, zero binary tests and
zero doctests. Help succeeded and lists `build`, `run`, `test`. Formatting was
checked separately; see [validation notes](tests/README.md) for its status.
The terminal shell also emits a tty-process-group error after commands; distinguish
that wrapper failure from Cargo's own output. Offline mode requires a populated
cache and is not necessary for normal development.

Useful focused commands: `cargo test --no-default-features frontend::parser`,
`cargo test --no-default-features frontend::tokenizer`, and
`cargo test --no-default-features --lib`. These filters select Rust unit tests,
not Fib program compilation.

## Assets

- [`src/`](../../src): active modules plus explicitly uncompiled legacy code.
- [`samples/`](../../samples): current-syntax examples with automatic parser coverage,
  not an executable regression suite.
- [`std/`](../../std): experimental Fib sources, not automatically checked.
- No root `tests/` directory exists; [test guidance](tests/README.md) retains its old URL.
- [`CI`](.github/workflows/ci.md) records configured gates, not proof they pass.
- [`.githooks/pre-commit`](../../.githooks/pre-commit) is opt-in; it skips docs-only
  staged changes and runs `cargo deny check` as well as frontend/full gates.
  Unlike advisory CI deny, local deny failure blocks the hook.

The planned project model uses explicitly named files in `BuildPlan`, optionally
produced by interpreting `build.fib`. It is not the former include-root/recursive
import model and is not implemented yet.
