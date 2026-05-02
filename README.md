# Fib

Fib is a systems programming language designed for performance, clarity, and developer control. 

## Compiler

The compiler is written in Rust. Future plans include a self-hosted compiler.

The compiler has two backends.
The first is an LLVM backend, which is the default setting (for now).
The second is a custom backend made for specialized builds.

## Installation

### Prerequisites

- **Rust toolchain** (stable): install via [rustup](https://rustup.rs)
- **clang-17**: must be on `$PATH` in case of using the LLVM backend.

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

## Language Features

- **Types**: `int8/16/32/64`, `uint8/16/32/64`, `float32/64`, `bool`, `char`, `string`, structs, arrays, raw pointers
- **Functions**: declarations, forward declarations, variadic, extern
- **Control flow**: `if`/`else`, C-style `for`, `break`, `continue`, `defer`, `return`
- **Expressions**: arithmetic, bitwise, logical, comparison, compound assignment, type casting (`as`), field/index access, address-of (`.&`), dereference (`.*`), struct construction, array literals
- **Memory**: manual `malloc`/`free`, pointer arithmetic, defer for cleanup

See `docs/` for language documentation and `samples/` for working example programs.

## Contributing

Contributions, suggestions, and bug reports are welcome. See [CONTRIBUTING](CONTRIBUTING) for details.

## License

MIT License. See [LICENSE](LICENSE) for details.
