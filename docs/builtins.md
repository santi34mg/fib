# Builtins

Everything built into the compiler is spelled with a leading `@`. This includes the
[built-in types](types.md) (`@int`, `@string`, ...) and the builtin **functions** below.
A name without the `@` is an ordinary identifier — there is no implicit builtin namespace.

## String functions

| Builtin | Signature | Description |
|---------|-----------|-------------|
| `@str_len` | `@str_len(s: @string) @uint8` | Number of bytes in `s` (libc `strlen`). |
| `@str_eq` | `@str_eq(a: @string, b: @string) @bool` | `true` when `a` and `b` are byte-for-byte equal (libc `strcmp`). |
| `@concat` | `@concat(a: @string, b: @string) @string` | A newly heap-allocated string holding `a` followed by `b`. |

All three take `@string` arguments; passing any other type, or the wrong number of
arguments, is a compile-time error.

> **Memory:** `@concat` allocates with `malloc` and does not free anything. The caller
> owns the returned string and is responsible for freeing it (e.g. `libc::free`).

### Example

```fib
import std::libc

fn main() @int4 {
    greeting: @string = @concat("Hello, ", "fib!")
    libc::printf("%s (len=%d, eq=%d)\n",
        greeting,
        @str_len(greeting),
        @str_eq(greeting, greeting))
    return 0
}
```

Output:

```
Hello, fib! (len=11, eq=1)
```

See [`samples/string_builtins.fib`](../samples/string_builtins.fib) for the runnable program.
