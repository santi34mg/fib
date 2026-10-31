# Arrays and slices

Array and slice types use prefix syntax: `[N]T` and `[]T`.
The [chosen value/mutability rules](../language-specification.md#values-initialization-and-mutability)
keep fixed arrays inline: binding const forbids whole-array replacement and
element writes; type-level const permits replacement but forbids element writes. Type/index disambiguation
is implemented. Native fixed-array literals, storage, copies, assignment, calls
and `.len` work, as do checked runtime indexing and slicing. Implicit fixed-array
to slice conversion in expected-type contexts is semantic behavior, not additional
parser syntax; see the linked language specification.

> **Verified parser behavior:** Array/slice types, literals, indexing, and range syntax parse.

## Types and literals

```fib
items: [4]int32 = [10, 20, 30, 40];
view: []int32 = items[0..4];
empty := [];
```

`[N]T` records an element type and integer-literal size; `[]T` records a slice
type. The size cannot be an identifier or arithmetic expression. Array literals
contain expressions in brackets; comma-separated elements and a trailing comma
parse. Empty array literals also parse.

Sentinel forms are `[N:S]T`, `[:S]T` and `[*:S]T`, with an integer-literal
sentinel S in 0–255. Analysis currently requires uint8 elements. `*[:S]T`
is a pointer to a slice descriptor, distinct from `[*:S]T`.

## Indexing and assignment

```fib
first := items[0];
items[i] = items[j];
```

The parser records the object and index.

Plain indexed assignment is accepted, while `items[i] += 1;` is rejected.
This is a **documented parser limitation, not a language design decision**, tracked
in [issue #15](https://github.com/santi34mg/fib/issues/15). The accepted
plain-assignment form is not a prescribed language-level workaround.

## Slice ranges

| Parsed form | Recorded bounds |
|-------------|--------------|
| `items[a..b]` | Start included, end excluded |
| `items[a.=b]` | Start and end included |
| `items[a..]` | Omitted end |
| `items[..b]` | Omitted start, exclusive end |
| `items[.=b]` | Omitted start, inclusive end |
| `items[..]` | Both bounds omitted |

```fib
middle := items[1..3];
inclusive := items[1.=2];
tail := items[1..];
prefix := items[..2];
all := items[..];
```

The AST stores optional bounds and an `inclusive` flag. Inclusive ranges require
an end: `items[.=]` and `items[1.=]` are rejected. `..=` is not the inclusive
separator. `items[]` also forms a full-range view; slice types use prefix `[]T`.

## Length member syntax

```fib
length := items.len;
```

`.len` parses as ordinary member access. 

The [language specification](../language-specification.md#declarations-generics-and-lexical-scope)
defines prefix type syntax, implicit array-to-slice conversion and borrowing rules.

Related follow-up is collected in this [GitHub issue #13](https://github.com/santi34mg/fib/issues/13).

Parser coverage: `test_array_literal`, `test_array_type_still_parses_with_size`,
`test_slice_type_expression`, `test_slice_range_forms`,
`test_slice_inclusive_needs_end`, and `test_len_property_after_dot`.
