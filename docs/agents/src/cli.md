# Binary and CLI

[`main.rs`](../../../src/main.rs) calls `Command::parse()`, gathers
`env::vars_os()`, and delegates to `exec_command`. The Clap schema in
[`cli.rs`](../../../src/cli.rs) accepts:

```sh
cargo run --no-default-features -- --help
cargo run --no-default-features -- build --help
# Accepted syntax only, NOT working compilation:
cargo run --no-default-features -- build --single-file samples/minimal.fib
```

The Rust binary target is `fibc`; the Clap declaration names the tool `fib`.
Top-level help was verified locally and displays `fibc` from invocation.
Commands are `build` (optional `--single-file PATH`), `run`, and `test`.
`exec_command` is `todo!()`; invoking an actual command panics rather than builds,
runs, or tests Fib code. Help is handled by Clap before dispatch.

The former positional-file CLI, `-I`, `--check`, `--emit`, `--cc`, `-O`,
`--release`, output and retained-LLVM switches are not current options. There
are no CLI unit or subprocess contract tests in the current module.
