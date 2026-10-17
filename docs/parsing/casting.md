# Casting

> **Verified parser behavior:** `as` builds a cast AST node with a type-expression target.

## Syntax

```fib
wide := value as int64;
byte := value as uint8;
pointer := buffer as *Node;
address := pointer as usize;
remote := value as other::Number;
```

The target is a type expression, including named/qualified types, pointers,
arrays, slices, tuples, structs, enums, and function types. Types have no `@`
prefix; widths are bits (`uint8` is eight bits).

## Precedence

Casts bind less tightly than shifts and arithmetic, but more tightly than
comparisons. Thus `a + b as int64` parses as `(a + b) as int64` and
`a as int64 == b` compares the cast expression with `b`. Parenthesize to make
another grouping explicit.

```fib
sum := a + b as int64;
converted_first := (a as int64) + b;
chained := value as int32 as int64;
```

Repeated `as` operators wrap the preceding expression from left to right.

## Related follow-up

Open conversion-design questions belong in the [roadmap](../roadmap/).

See [Operators](operators.md), [Types](types.md), and
[Pointers and memory](pointers-memory.md). Related follow-up is collected in this [GitHub issue #11](https://github.com/santi34mg/fib/issues/11).

Parser coverage includes
`test_cast_expression` and `test_builtin_types_do_not_use_at_prefix`.
