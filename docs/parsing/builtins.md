# Builtins

> **Verified parser behavior:** The tokenizer/parser recognizes the builtin call spellings below.

## Types versus functions

Builtin **types** are bare names: `int32`, `string`, `bool`, `usize`, etc.
`@int32` and `@void` are rejected with a diagnostic saying types do not use `@`.
See [Types](types.md) for widths and migration from old byte-size names.

The three builtin **functions** are recognized in both bare and `@`-prefixed
form. Both forms produce a `CallToBuiltin` AST node:

```fib
a := concat("hello", " world");
b := @concat("hello", " world");
n := str_len(a);
m := @str_len(b);
same := str_eq(a, b);
equal := @str_eq(a, b);
```

| Function | Intended operation, not a checked signature |
|----------|---------------------------------------------|
| `concat` / `@concat` | String concatenation |
| `str_len` / `@str_len` | String byte length |
| `str_eq` / `@str_eq` | String equality |

Parentheses are required, including for zero arguments. A bare builtin function
name is not a first-class function reference. These bare spellings are builtin
tokens, so they cannot be declared as ordinary function/variable/field names.
Unknown `@name` calls are rejected.

The parser records argument expressions without validating a builtin signature.
Builtin spelling proposals are tracked in the [roadmap](../roadmap/).

## Length member syntax

```fib
n := items.len;
```

`.len` parses as ordinary member access. `items.@len` and bare `@len` are
rejected; `len` itself is an ordinary identifier, not a builtin function.

Parser coverage: `test_builtin_call_shape`, `test_builtin_call_missing_paren_errors`,
`test_unknown_builtin_surfaces_lexer_message`, `test_len_property_after_dot`, and
`test_bare_len_property_errors` in [`src/frontend/parser/test.rs`](../../src/frontend/parser/test.rs).
