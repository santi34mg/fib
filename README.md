# Fiber

This is a compiler project for fib, a language designed for performance, clarity, and control.
The goal is to create a language and toolchain that combines low-level control with safer abstractions and ergonomics suited for systems programming and backend development.

The compiler, called fibc, is written in Rust but aims to be self-hosted.

## Current Status

The compiler frontend is advancing with support for expressions, variable declaration statements, function declarations, and returns.
The current compiler backend lowers to LLVM IR, although a multi-backend strategy is planned for the future (like Zig with LLVM and in house backend).

## Code quality and performance

I will admit that this is not the best Rust out there.

The Rust version of the compiler aims to be a bootstrap, therefore, performance and code quality is not an objective.

## Getting Started

You can clone the repo and compile the whole toolchain using `cargo install --path cli`.
This will leverage cargo in order to install the toolchain.

Then, use `fiber init <path>` to initalize a project, cd into it and run `fiber compile` in order to produce a binary.

## Contributing

Contributions, suggestions, and bug reports are welcome.
This project is a personal exploration of compiler design and language implementation, but any help or feedback is appreciated.

## License

MIT License. See [LICENSE](LICENSE) for details.
