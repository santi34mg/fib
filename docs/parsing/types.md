# Types

> **Verified parser behavior:** The names and composite forms below are recognized as type syntax.

## Built-in names

Types do **not** use `@`. Numeric suffixes are widths in **bits**, not bytes.

| Names | Parsed meaning |
|-------|----------------|
| `int8`, `int16`, `int32`, `int64`, `int128` | Signed integer types |
| `uint8`, `uint16`, `uint32`, `uint64`, `uint128` | Unsigned integer types |
| `int`, `uint` | Aliases for `int32`, `uint32` |
| `float16`, `float32`, `float64`, `float128` | Floating-point types |
| `bool` | Boolean type |
| `char` | Character type |
| `string` | String type |
| `void` | No-value type; default function return type |
| `never` | Never type |
| `usize` | Intended pointer-width unsigned size type |
| `type` | Metatype, also used to introduce type declarations |

These spellings are builtin tokens, not ordinary identifiers: they cannot be
used as variable, parameter, field, or variant names.

Old names such as `@int4`, `@void`, and `@uint8` are not current type syntax.
Use `int32`, `void`, and `uint64` respectively; do not mechanically remove `@`
from old byte-width names (`uint8` now means eight bits).

## Composite type expressions

```fib
type Node struct { value: int32, next: *Node }
type Result enum { Ok { value: int32 }, Empty }
type Pair (int32, bool)
type Callback fn(int32, int32) -> int32
type Buffer uint8[16]
type View uint8[]
type Address *void
type Remote other::Thing
```

- `*T` is pointer syntax. No ownership/borrowing pointer kinds are parsed.
- `T[N]` parses a fixed array; `N` must be an integer literal, not an expression.
- `T[]` parses a slice.
- `(A, B)` parses a tuple type; `(A)` is just `A`, and `()` is rejected.
- `fn(A, B) -> R` is function **type** syntax. Function **declarations** have no
  arrow; see [Functions](functions.md).
- Qualified names such as `other::Thing` parse as named type references.

The type parser consumes one array/slice suffix at each recursive level, not
an arbitrary suffix chain. Do not assume `int32[2][3]` is supported. Pointer
pointees are parsed recursively: `*int32[3]` is a pointer to an array.

## Named types and type values

Current top-level declaration syntax is `type Name TypeExpression`, without
`=` or a trailing semicolon. This applies to structs/enums and aliases alike.
Parsing alone does not decide whether a name introduces a distinct type or an
alias. See the design discussion in [Name resolution](../name-resolution.md)
and the [roadmap](../roadmap/).

```fib
type Count int32
fn identity(T: type, value: T) T { return value; }
```

The latter preserves a metatype parameter and named type references in the AST;
it does not implement generics. See [Generics](generics.md). The planned
`comp Name: type = ...;` model is not parsed; see
[Name resolution](../name-resolution.md).

Parser coverage: `builtin_type_spellings_have_expected_ast_variants`,
`composite_type_expressions_preserve_nesting`, and
`metatype_token_works_in_declarations_annotations_and_expressions` in
[`src/frontend/parser/test.rs`](../../src/frontend/parser/test.rs).
