# Fib Language Documentation

Fib is a systems programming language designed for performance, clarity, and developer control. The pages below cover each feature of the language briefly with example syntax.

## Contents

- [Types](types.md) — built-in primitive types
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
- [Imports and Modules](imports.md) — `import`, qualified access, selective imports, aliasing
- [Casting](casting.md) — `as` operator
- [Generics via Type Parameters](generics.md) — `T type` parameters

For working programs that exercise these features end-to-end, see the [`samples/`](../samples) directory.
