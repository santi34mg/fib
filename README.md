# Fiber

Fib is a systems programming language designed for performance, clarity, and developer control. 

Fiber is fib's official toolchain.

## Compiler

The compiler **`fibc`** is written in Rust. Future plans include a self-hosted compiler.

The compiler has two backends.
The first is an LLVM backend, which is the default setting (for now).
The second is a custom backend made for specialized builds.

## Compilation Pipeline

`fiber` tool allows project wide compilation while `fibc` can be used for single source file compilation.

```
Source (.fib) -> Lexer -> Tokens -> Parser -> AST -> Analysis -> HIR -> Backend -> Binary
```

## Prerequisites

- **Rust toolchain** (stable): install via [rustup](https://rustup.rs)
- **clang-17**: must be on `$PATH` in case of using the LLVM backend.

## Getting Started

```bash
# Install the fiber CLI
cargo install --path cli

# Create a new project
fiber init my_project
cd my_project

# Compile and run
fiber compile
./out/main
```

## Language Features

- **Types**: `int8/16/32/64`, `uint8/16/32/64`, `float32/64`, `bool`, `char`, `string`, structs, arrays, raw pointers
- **Functions**: declarations, forward declarations, variadic, extern
- **Control flow**: `if`/`else`, C-style `for`, `break`, `continue`, `defer`, `return`
- **Expressions**: arithmetic, bitwise, logical, comparison, compound assignment, type casting (`as`), field/index access, address-of (`.&`), dereference (`.*`), struct construction, array literals
- **Memory**: manual `malloc`/`free`, pointer arithmetic, defer for cleanup

See `docs/` for language documentation and `samples/` for working example programs (hello world, linked list, memory pool, Fibonacci benchmark, bitwise ops, insertion sort).

## Editor Support

An LSP server (`fiber-lsp`) provides diagnostics, hover, and go-to-definition. Build it with:

```bash
cargo build -p fiber-lsp
```

## Project Layout

```
fibc/        — core compiler library and binary (lex, parse, analyze, lower)
cli/         — user-facing `fiber` CLI (init, compile, deps)
fiber-lsp/   — LSP server (frontend only, no LLVM dependency)
std/         — standard library prelude (libc.fib)
samples/     — example Fiber programs
docs/        — language documentation
```

## Contributing

Contributions, suggestions, and bug reports are welcome. This project is a personal exploration of compiler design and language implementation.

## License

MIT License. See [LICENSE](LICENSE) for details.
