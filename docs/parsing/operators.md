# Operators

> **Verified parser behavior:** The table describes expression parsing and grouping.

## Precedence (tightest to loosest)

| Level | Syntax |
|-------|--------|
| Primary/postfix | Grouping `(expr)`, calls `callee(args)`, member `.field`, index `[i]`, slice `[a..b]`, restricted postfix `*` / `&` |
| Prefix unary | `-`, `!`, `~` |
| Multiplicative | `*`, `/`, `%` |
| Additive | `+`, `-` |
| Shifts | `<<`, `>>` |
| Cast | `as Type` |
| Relational | `<`, `<=`, `>`, `>=` |
| Equality | `==`, `!=` |
| Bitwise AND | `&` |
| Bitwise XOR | `^` |
| Bitwise OR | `|` |
| Logical AND | `&&` |
| Logical OR | `||` |

Binary levels are left-associative. In particular, equality binds **more tightly**
than bitwise AND: use `(flags & mask) != 0` when that is the intended grouping.
Casts wrap arithmetic/shift expressions unless grouping says otherwise.

```fib
arithmetic := 1 + 2 * 3;
shifted := 1 << 2 + 3;
masked := (flags & mask) != 0;
logical := a && b || c;
converted := (value as int64) + other;
```

The grammar represents logical operators as binary nodes. Unary `+`, prefix
pointer operators, `++`, and `--` are not implemented productions.

## Postfix syntax and its limits

```fib
address := value&;
loaded := pointer*;
field := (pointer*).field;
item := items[i];
view := items[a..b];
member := obj.inner.value;
```

No dot precedes pointer/index/slice operators. Old `value.&`, `pointer.*`, and
`items.[i]` forms are rejected. Pointer postfix `*` and `&` are currently consumed
only when the **next token** is `;`, `)`, `]`, `}`, or `,`. Otherwise they remain
binary multiplication/AND tokens. Use grouping to continue access or arithmetic:
`(pointer*).field` and `(value&) as *void`. Whitespace does not disambiguate them.
Postfix at end of input without a following delimiter is not recognized either.

Slice syntax uses `..` for exclusive ends and `.=` for inclusive ends, not `..=`.
Omitted bounds are parsed as described in [Arrays](arrays.md). Numeric tuple
member access such as `pair.0` explicitly reports that it is unimplemented.

## Assignment is statement syntax

```fib
count = 1;
obj.field = 2;
items[i] = 3;
count += 1;
count -= 1;
count *= 2;
count /= 2;
count %= 3;
```

Assignments are handled in name-starting statements rather than the general
expression grammar. Compound assignments accept a single unqualified name or
an indexed target, including nested indices and calls such as
`items[next_index()] += amount();`. Indexed compounds retain one target and RHS
with the arithmetic operator in a dedicated AST node to preserve single evaluation
in future lowering; analysis and execution are not implemented. Field and
qualified-name compound targets remain rejected. `:=` is
inferred-declaration syntax (tokenized as `:` and `=`), not a general expression
operator. Multiple assignment is not parsed. See [Variables](variables.md).

The compound-target restriction is a parser limitation, not a settled language
design rule.

## Other punctuation

- `::` separates unresolved static name segments; see [Imports](imports.md).
- `->` is consumed in function types, not function declarations.
- Variadic parameter syntax is removed; `...` is not a working spread operator.
- Former `@` builtin calls and types are rejected. Compiler intrinsics resolve
  through ordinary synthetic namespace declarations.

Parser coverage includes `test_binary_levels_are_left_associative`,
`test_binary_shift_binds_looser_than_additive`, `test_binary_logical_or_loosest`,
`test_postfix_access_keeps_binary_operators`, and
`compound_assignment_rejects_unrelated_target_forms`, and
`indexed_compound_assignment_preserves_each_expression_once`.
