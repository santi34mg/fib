# Fib

[![CI](https://github.com/santi34mg/fib/actions/workflows/ci.yml/badge.svg)](https://github.com/santi34mg/fib/actions/workflows/ci.yml)

Fib is a systems programming language under active development, written in Rust.
Performance, clarity, and developer control are the design goals.

See the [language documentation](docs/README.md) for current parsed syntax and
[compiler architecture](docs/compiler-architecture.md) for implementation boundaries. The [language specification](docs/language-specification.md)
defines the language rules; implementation status is documented separately.

Start with the [runnable example project](example-project/README.md) to try the
current scalar types, casts, bit operations, helpers, recursion and loops.
The executable [specification sample](samples/specification_runtime.fib) also
combines generic specialization, array values, bool casts, pointer access and
exhaustive switches.

## Installation

Install compiler binaries and matching standard-library sources together:

```sh
cargo run --no-default-features --bin fib-install -- --prefix "$HOME/.local"
```

For development, use `cargo run -- build --sysroot library --single-file samples/syscalls.fib`.
See [standard-library setup and contracts](docs/standard-library.md), including
`--no-std` and LSP configuration.

## Contributing

The Rust compiler is a [Cargo workspace](crates/README.md). Root `cargo test`
selects every compiler crate; `cargo test -p fib-frontend` and
`cargo test -p fib-ir` run individual stages without LLVM.

Contributions, suggestions, and bug reports are welcome. See
[CONTRIBUTING](CONTRIBUTING) for development and PR guidance.

## License

MIT License. See [LICENSE](LICENSE).
