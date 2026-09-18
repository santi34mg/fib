# Fib

[![CI](https://github.com/santi34mg/fib/actions/workflows/ci.yml/badge.svg)](https://github.com/santi34mg/fib/actions/workflows/ci.yml)

Fib is a systems programming language designed for performance, clarity, and developer control. 

## Compiler

The compiler is written in Rust. Future plans include a self-hosted compiler.

The compiler has two backends.
The first is an LLVM backend, which is the default setting (for now).
The second is a custom backend made for specialized builds.

## Installation

### Prerequisites

- **Rust toolchain** (stable): install via [rustup](https://rustup.rs)
- **clang**: must be on `$PATH` in case of using the LLVM backend (`clang-17` is preferred when installed; otherwise unversioned `clang` is used).

### Building from source

```bash
git clone https://github.com/santi34mg/fib.git
cd fib

cargo build
```

### Running hello world

To run any of the samples you can do so directly with cargo from the project root:

```
cargo run -- samples/hello_world.fib -I=std
./out/hello_world
```

`-I`/`--include-path` adds an import search directory (repeatable). The entry
file's own directory is always searched first, so `-I=std` is what lets
`import std::libc` resolve. `-o` overrides the default `out/<stem>` binary
path.

Useful flags (see `cargo run -- --help` for the full list):

- `--check` — frontend only (lex + parse + analyze), no LLVM/clang needed.
- `--emit=lex|parse|typed|llvm|bin` — stop after a stage.
- `--emit-llvm` / `--llvm-out <FILE>` — keep the intermediate `.ll`.
- `--cc <CC>` — C compiler for linking (defaults to `$CC`, else `clang-17`, else `clang`).
- `-O <LEVEL>` — optimization level passed to clang as `-O<LEVEL>`.

### Troubleshooting

- `clang-17: not found` / linker errors: install a C compiler and ensure it
  is on `$PATH`. Any recent `clang` works (`clang-17` is just preferred when
  present); or point at yours explicitly: `cargo run -- samples/hello_world.fib -I=std --cc clang`.
- Building the compiler itself needs LLVM 21 (CI installs it via
  `ZhongRuoyu/setup-llvm@v0`). Frontend-only work (`--check`,
  `--emit=lex|parse|typed`, `cargo check --no-default-features`) needs
  neither LLVM nor clang.
- No output binary? The default `--emit=bin` writes to `out/<stem>`
  (`out/` is git-ignored); use `-o` to choose another path.

## Language Features

- **Types**: built-in types are `@`-prefixed — byte-sized integers (`@int`, `@int2`, `@int4`, `@int8`, `@int16` and unsigned `@uint...` counterparts), `@float4/8/16`, `@bool`, `@char`, `@string`, plus structs, enums/tagged unions, arrays, raw pointers
- **Builtins**: `@`-prefixed builtin functions for string building — `@concat`, `@str_len`, `@str_eq`
- **Functions**: declarations with `name: type` parameters, forward declarations, multiple return values, variadic, extern, type parameters (generics)
- **Variables**: explicit declarations use `name: type = init`; inferred declarations use `name := init`
- **Control flow**: `if`/`else`, C-style `for`, `break`, `continue`, `defer`, `return`, `switch`/`when` pattern matching
- **Expressions**: arithmetic, bitwise, logical, comparison, compound assignment, type casting (`as`), field/index access, address-of (`.&`), dereference (`.*`), struct construction, array literals, `null`
- **Memory**: manual `malloc`/`free`, pointer arithmetic, defer for cleanup
- **Modules**: `import a::b`, selective imports `::{X}`, aliasing `as`

See `docs/` for language documentation and `samples/` for working example programs.

## Contributing

Contributions, suggestions, and bug reports are welcome. See [CONTRIBUTING](CONTRIBUTING) for details.

## License

MIT License. See [LICENSE](LICENSE) for details.
