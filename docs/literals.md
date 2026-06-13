# Literals and Comments

## Comments

Line comments start with `//` and run to the end of the line. There are no block comments.

```fib
// compute the length of the list
len: @int4 = list_length(head)   // trailing comments work too
```

## Integer literals

Decimal by default. A `0`-prefix selects another base:

| Prefix | Base | Example |
|--------|------|---------|
| (none) / `0d` | decimal | `42`, `0d42` |
| `0x` | hexadecimal | `0xFF` |
| `0o` | octal | `0o17` |
| `0b` | binary | `0b1010` |

## Float literals

A decimal point with digits on both sides produces a float:

```fib
f: @float8 = 3.25
```

Only decimal floats are supported. `1..5` lexes as `1`, `..`, `5` (the `..` token is reserved), so a fractional part requires a digit after the dot.

## Boolean literals

`true` and `false`.

## Character literals

Single quotes: `'a'`. Escape sequences:

- `\n`, `\t`, `\0`, `\\`, `\'`, `\"`
- `\xNN` — two hex digits (byte value)
- `\u{...}` — Unicode code point in hex, e.g. `'\u{41}'`

## String literals

Double quotes: `"hello"`. Escape sequences: `\n`, `\t`, `\r`, `\0`, `\\`, `\"`, `\'`.

```fib
libc::printf("line one\nline two\n")
```

## `null`

The null pointer literal — see [Pointers and Memory](pointers-memory.md).

## Reserved identifiers

Identifiers beginning with `__` (double underscore) are reserved for compiler internals.
