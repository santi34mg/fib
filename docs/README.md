# Fib Language Documentation

Fib is a systems programming language designed for performance, clarity, and developer control. The pages below cover each feature of the language briefly with example syntax.

## Compiler status

The current branch contains parser fixes and architecture sketches, not an
executable compilation pipeline. See [Compiler Architecture](compiler-architecture.md)
for the `AnalyzedProject → Ir` contract, implemented pieces, and deferred work.

## Contents

- [Types](types.md) — built-in primitive types (`@`-prefixed)
- [Builtins](builtins.md) — `@`-prefixed builtin functions (`@concat`, `@str_len`, `@str_eq`)
- [Literals and Comments](literals.md) — integer bases, floats, chars, strings, escapes, `null`, comments
- [Variables](variables.md) — declarations, type inference, immutability
- [Operators](operators.md) — arithmetic, bitwise, logical, comparison, compound assignment
- [Control Flow](control-flow.md) — `if`/`else`, `for`, `break`, `continue`, `defer`, `return`
- [Functions](functions.md) — declarations, parameters, return types, multiple returns, forward declarations, variadic, `extern`
- [Structs](structs.md) — record types, construction, field access
- [Enums and Tagged Unions](enums.md) — discriminated enums and variants with payloads
- [Switch](switch.md) — pattern matching over enums and tagged unions
- [Arrays](arrays.md) — fixed-size arrays, indexing, array literals
- [Pointers and Memory](pointers-memory.md) — raw pointers, address-of, dereference, manual allocation
- [Imports and Modules](imports.md) — current `import` behavior and qualified access
- [Planned Name Resolution](name-resolution.md) — proposed file namespaces, `comp` aliases, visibility, and function pointers (not implemented)
- [Casting](casting.md) — `as` operator
- [Generics via Type Parameters](generics.md) — `T type` parameters

The [`samples/`](../samples) directory contains language examples with expected-output
comments from the previous pipeline. They do not establish end-to-end support in
the current architecture-only branch.

Compiler contributors and automated agents should start with the
[`implementation/`](implementation/README.md) tree. It mirrors the repository
layout and records pipeline ownership, implementation invariants, known risks,
and prioritized improvements.
