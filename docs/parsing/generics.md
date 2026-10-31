# Generics: parsed metatype syntax

> **Verified parser behavior:** Metatype parameters and builtin type values parse as ordinary signature and expression nodes.

## What currently parses

```fib
fn identity(T: type, value: T) T {
    return value;
}
result := identity(int32, 42);
```

- `T: type` is recorded as an ordinary parameter whose type is the builtin
  metatype.
- `T` in parameter and return type positions is an unresolved qualified-name
  type expression.
- `int32` in the call is a builtin `TypeValue` expression.
- The call is an ordinary call node; there is no parser-side instantiation.

A builtin type may appear as an expression argument, while a user-defined name
in expression position remains an unresolved name. Resolution must determine
whether it denotes a type, value, or invalid argument.

```fib
comp Count: type = int32;
fn first(T: type, values: *T) T {
    return values[0];
}
result := first(Count, data);
```

Ordinary type syntax also permits named types in local annotations, casts,
arrays, and slices.

## Parser metadata and design links

The parser always initializes `FunctionSignature.generic_parameters` to an
empty vector. It does not classify `type` parameters into generic metadata.

Ordinary metatype parameters and type-value arguments provide generic
specialization. Analysis performs specialization and concrete
body rechecking; functions computing types require explicit `comp fn`. This
parser snapshot alone does not establish semantic or executable support. See
[type resolution](../type-resolution.md) and the [roadmap](../roadmap/README.md).

See [Functions](functions.md) and [Types](types.md). Related follow-up is collected in this [GitHub issue #12](https://github.com/santi34mg/fib/issues/12).

Parser evidence includes
`metatype_token_works_in_declarations_annotations_and_expressions`.
