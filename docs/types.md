# Types

Built-in types are **prefixed with `@`** — `@int`, `@string`, `@bool`, etc. The `@`
marks a name as belonging to the compiler's builtins (see [Builtins](builtins.md));
a bare name like `int` is just an ordinary identifier. Fib's built-in numeric types
use a byte-size suffix: `@int4` is a 4-byte (32-bit) signed integer, `@uint8` is an
8-byte (64-bit) unsigned integer, etc.

## Primitive types

| Type | Description |
|------|-------------|
| `@void` | No value (used as a return type) |
| `@bool` | Boolean (`true` / `false`) |
| `@char` | Single character |
| `@string` | String value |
| `@int`, `@int2`, `@int4`, `@int8`, `@int16` | Signed integers (1, 2, 4, 8, 16 bytes) |
| `@uint`, `@uint2`, `@uint4`, `@uint8`, `@uint16` | Unsigned integers (1, 2, 4, 8, 16 bytes) |
| `@float4`, `@float8`, `@float16` | Floating-point numbers (4, 8, 16 bytes) |
| `@never` | A value that can never be produced (e.g. functions that never return) |

## Composite types

- **Pointers**: `*T` (raw pointer to `T`) — see [Pointers and Memory](pointers-memory.md)
- **Arrays**: `T[N]` (fixed-size array of `N` elements of type `T`) — see [Arrays](arrays.md)
- **Structs**: `struct { ... }` — see [Structs](structs.md)
- **Enums / tagged unions**: `enum { ... }` — see [Enums](enums.md)
- **Tuples**: appear in multi-return signatures, e.g. `(@int4, @int4)` — see [Functions](functions.md)
- **Function types**: `fn(@int4, @int4) -> @int4` — parsed but reserved; first-class function values are not usable yet ([Functions](functions.md))

A type imported from another module is referenced with a qualified name, e.g. `error::Error` — see [Imports](imports.md).

## Type aliases

Use `type` to give a name to a type expression:

```fib
type Color enum { Red, Green, Blue }

type Pool struct {
    start: @uint8,
    capacity: @uint8,
}
```
