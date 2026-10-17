# Fib

[![CI](https://github.com/santi34mg/fib/actions/workflows/ci.yml/badge.svg)](https://github.com/santi34mg/fib/actions/workflows/ci.yml)

Fib is a systems programming language under reconstruction, written in Rust.
Performance, clarity, and developer control are design goals, not a claim of a
finished compiler.

## Current implementation

Tokenization, parsing, AST construction, lexical scope utilities, diagnostics,
and raw IR data utilities work independently. Semantic analysis, build-file
execution, AST-to-IR lowering, SSA building/verification, IR interpretation,
and backend emission remain unfinished. There is no working end-to-end compiler
or executable custom backend in this checkout.

See the [language documentation](docs/README.md) for current parsed syntax and
[compiler architecture](docs/compiler-architecture.md) for implementation boundaries.
Contributors and coding agents should start with [AGENTS.md](AGENTS.md) and the
[implementation guide](docs/agents/README.md).

## Build and test

The repository pins Rust **1.98.0** in `rust-toolchain.toml`. Install it through
[rustup](https://rustup.rs). For development without LLVM:

```sh
git clone https://github.com/santi34mg/fib.git
cd fib
cargo check --no-default-features
cargo test --no-default-features
cargo run --no-default-features -- --help
```

Use `--offline` when dependencies are already cached and network access is not
available. The default `llvm` feature enables Inkwell 0.8 / LLVM 21 dependency
builds and requires a compatible LLVM installation. Enabling it does not enable
working code generation.

The CLI accepts `build [--single-file PATH]`, `run`, and `test`, but executing
these commands is unimplemented. Help works. Historical positional source-file,
include-root, check, emit, linker, and release flags are not supported. All 15
sample files are checked by an automatic parser test, not compiled or executed.
Standard-library files may still contain obsolete syntax and are not swept by
that test.

## Current syntax

This example illustrates parsing, not successful compilation or execution:

```fib
type Point struct { x: int32, y: int32 }

fn add(a: int32, b: int32) int32 {
    const offset := 1;
    result: int32 = a + b + offset;
    while (result < 10) {
        result += 1;
    };
    return result;
}
```

- Builtin types use bare bit-width names: `int8` through `int128`, `uint8`
  through `uint128`, `float16` through `float128`, plus `bool`, `char`,
  `string`, `void`, `never`, `usize`, and `type`. `int` and `uint` alias 32-bit types.
- Bindings use `x := value`, `x: T = value`, or `x: T`, with optional `const`.
- Control flow includes `if`, `while`, and `switch`; statements require semicolons.
- Indexing and slicing use `a[i]` and `a[a..b]`; length access uses `.len`.
- Builtin functions retain `@`: `@concat`, `@str_len`, and `@str_eq`.
- Imports, module loading, semantic generic specialization, runtime memory
  behavior, and executable lowering are not implemented.

The [language reference](docs/README.md) describes parser restrictions and future
features separately. The dependency-free [LSP](lsp/README.md) supplies syntax-based
semantic highlighting, not compiler diagnostics.

## Contributing

Contributions, suggestions, and bug reports are welcome. See
[CONTRIBUTING](CONTRIBUTING) for development and PR guidance.

## License

MIT License. See [LICENSE](LICENSE).
