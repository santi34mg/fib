# Primitive types and ordinary function calls

> **Verified parser behavior:** primitive types use bare names; functions use ordinary identifiers.

Types such as `int32`, `string`, `bool`, `usize` and `type` are recognized by the
compiler. `@int32` and `@void` are rejected. See [Types](types.md).

There are no builtin function tokens or builtin-call AST nodes. `concat`,
`str_len` and `str_eq` are ordinary identifiers and can be declared, referenced,
passed as function values or used as field names. Calls use the same `Call`
AST as any other function:

```fib
fn concat(a: int32, b: int32) int32 { return a + b; }
answer := concat(1, 2);
```

The compiler does not supply these functions or prescribe their signatures.
String helpers will be implemented by the standard library. Former `@concat`,
`@str_len` and `@str_eq` spellings are rejected with source locations.

## Length member syntax

```fib
n := items.len;
```

`.len` parses as ordinary member access. `items.@len` and bare `@len` are
rejected; `len` is an ordinary identifier. The type pass distinguishes actual
struct fields from array/slice/string length. See [type resolution](../type-resolution.md).

Coverage includes `former_builtin_function_names_are_identifiers`,
`former_builtin_functions_are_ordinary_calls`, `removed_builtin_sigils_are_rejected`,
`test_len_property_after_dot` and `test_bare_len_property_errors`.
