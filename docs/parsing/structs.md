# Structs

> **Verified parser behavior:** Struct type expressions, named constructions, field expressions, and name-starting field assignments parse.

## Declarations

```fib
type Point struct {
    x: int32,
    y: int32,
}
type Node struct {
    value: int32,
    next: *Node,
}
```

Fields are `name: Type`; comma-separated lists and trailing commas are accepted.
The top-level declaration is `type Name struct { ... }`, without `=` or a
trailing semicolon. A pointer reference to the type being declared also parses.

The parser also accepts anonymous struct type expressions in annotations;
parsing does not supply an anonymous struct-value construction syntax.
`comp Name: type = struct { ... };` is a planned alternative declaration model,
not current grammar. See [Name resolution](../name-resolution.md).

## Named construction

```fib
point: Point = Point { x: 1, y: 2 };
remote := other::Point { x: 3, y: 4 };
```

Construction requires an identifier or qualified identifier immediately followed
by `{ field: expression, ... }`. Empty field lists and trailing commas parse.
There is no field shorthand, default-field expansion, or `...` spread syntax.

A dotted member followed by braces is not a supported construction form; in
particular old enum payload examples `Token.Integer { value: 7 }` do not parse.
See [Enums](enums.md).

## Access, assignment, and nesting

```fib
type Inner struct { value: uint64 }
type Outer struct { inner: Inner }
outer := Outer { inner: Inner { value: 0 } };
outer.inner.value = 42;
read := outer.inner.value;
address := outer&;
through_pointer := (pointer*).inner.value;
```

Each `.field` creates an unresolved member-access node. Field access through a
parsed pointer dereference requires grouping because postfix `*` is recognized
before `)` but not before `.`. Old `pointer.*.field` syntax is rejected.

A plain field assignment parses when the statement begins with a name;
`outer.inner.value += 1;` does not, because compound assignment is restricted to
single unqualified names. This is a parser limitation, not a settled design rule.

The parser disambiguates name conditions from struct constructions in `if`
conditions. A construction followed by access/comparison can also parse.

Parser coverage: `type_declarations_preserve_struct_and_enum_payload_shapes`,
`test_field_access`, `qualified_callee_and_field_access_preserve_identifiers`, and
`if_struct_condition_nested_field_else_if_and_continued_expression`.
