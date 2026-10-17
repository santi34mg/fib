# Verified parsing reference

This directory records **what the current tokenizer and parser accept and
reject during the compiler refactor**. It is an implementation snapshot, not
the intended language specification. A parser limitation is not a language-design
decision, and successful parsing is not semantic validation.

Analysis and execution are unfinished. Rather than repeating that limitation
throughout these pages, analysis work is tracked in the
[analysis issue inventory](../agents/issues/README.md). It links the published GitHub issues.

## Syntax inventory

- [Types](types.md)
- [Builtin calls](builtins.md)
- [Literals, identifiers, and comments](literals.md)
- [Bindings](variables.md)
- [Operators and assignments](operators.md)
- [Control flow](control-flow.md)
- [Functions](functions.md)
- [Structs](structs.md)
- [Enums](enums.md)
- [Switch patterns](switch.md)
- [Arrays and slices](arrays.md)
- [Pointers](pointers-memory.md)
- [Qualified paths and unsupported import syntax](imports.md)
- [Casts](casting.md)
- [Parsed generic-related forms](generics.md)

## Design and implementation

- [Documentation overview](../README.md)
- [Name-resolution design](../name-resolution.md)
- [Roadmap and proposals](../roadmap/README.md)
- [Compiler architecture](../compiler-architecture.md)
- [Agent implementation guide](../agents/README.md)

Use tokenizer/parser tests to validate changes to this snapshot. Use explicit
language-design decisions and semantic acceptance tests to determine the intended
language; do not infer those decisions from a temporary parser restriction.
