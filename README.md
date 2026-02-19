# Fiber

This is a compiler project for Fiber, a language designed for performance, clarity, and control.
The goal is to create a language and toolchain that combines low-level control with safer abstractions, ownership models, and ergonomics suited for systems programming and backend development.

The compiler, called Fiberc, is written in Rust but aims to be self-hosted.

## Current Status

At this stage, the compiler can read source files and tokenize input code into a stream of tokens.

The project is under active development and meant for learning, experimentation, and laying the groundwork for a modern systems programming language.

## Code quality

I will admit that this is not the best Rust out there.
Too many clones and Box.

I am just doing that in order to keep the program simpler and move faster to a point where the language can host itself and Rust is no longer needed.
If that point is reached, I will probably keep the Rust version in its own branch for anyone who wants to support/refactor it.

## Getting Started

TODO: fill in the getting started section of main README.

### Compile a `.fib` source file:

Currently, this will tokenize the input and print tokens to stdout as a debugging step.

## Contributing

Contributions, suggestions, and bug reports are welcome.
This project is a personal exploration of compiler design and language implementation, but any help or feedback is appreciated.

## License

MIT License. See [LICENSE](LICENSE) for details.
