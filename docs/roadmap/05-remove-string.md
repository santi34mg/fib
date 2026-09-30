# 05 — Remove the `string` Type

**Depends on:** [04](04-generic-types.md) — `Vec[char]` cannot be written
without generic types.

## Today

`@string` is an opaque `ptr`. A literal lowers to a private global
`[N+1 x i8]`, NUL-terminated by inkwell's `build_global_string_ptr`, and the
expression value is the pointer to element 0. There is no length anywhere.

## The new model

| Form | Type | Lowers to |
|---|---|---|
| `"hola"` | `[4]char` | global `[4 x i8]`, **no** NUL |
| `c"hola"` | `*char` | global `[5 x i8]` with NUL; value is the pointer |
| `[]char` | slice | `{ptr, i64}` — views over `[N]char` |
| `Vec[char]` | std generic struct | `{ptr, len, cap}`, owning, freeable |

`char` is already `i8` and already has literals (`'\n'`, `'\xNN'`, `'\u{…}'`),
so the foundation exists. `c"…"` is a new lexer form beside
`lex_string_literal` (`lexer.rs:306-351`).

## The FFI is the real work

**54 of the 153 `extern` declarations in `std/libc.fib` mention `@string`** (81
tokens), and every one depends on NUL termination: `printf`, `strlen`, `strcmp`,
`open`, `system`, `getenv`, `dlopen`, and the rest. All become `*char`. Every
format-string literal in the corpus becomes `c"…"` — mechanical but wide (13 in
`bitwise_ops.fib`, 10 in `unsigned_ops.fib`, and so on).

Two uses are conflated under `@string` today and must be separated:

- **input values** — `printf(fmt)`, `open(path)` → `*char`;
- **output buffers** — `getcwd`, `readlink`, `gethostname`, `snprintf`,
  `sprintf`, `strcpy` → also `*char`, but the caller allocates. `std` does this
  today with `libc::malloc(n) as @string`.

Nine externs declare `*@string` where C says `char*` (`strcpy`, `strdup`,
`getenv`). That extra indirection is harmless today because both lower to `ptr`;
once the element type is `*char` they become a genuine `**char` and must be
corrected.

## The 215 uses in `std`

Twenty files. Heaviest: `libc.fib` (54), `file.fib` (18), `str.fib` (15),
`serialization.fib` (9), `format.fib` (9), `encoding.fib` (8).

Two patterns change shape, not just spelling:

- **`@string` as a struct field or enum payload** (`serialization.fib`,
  `net.fib`, `core/error.fib`, `event.fib`, `io/fd.fib`) goes from 8 bytes to 16
  (`[]char`) or 24 (`Vec[char]`), **changing the payload computation of every
  tagged union** (`enum_max_payload_bytes`, `backend/lowering/types.rs:120-134`).
- **`format_bool` returns literals** `"true"`/`"false"`. Under `[N]char` those
  have different types (`[4]char`, `[5]char`), so the function must return
  `[]char` views over globals — no allocation, but it has to be written that way
  deliberately.

## An ICE to close here

`s1 == s2` currently compiles to a **pointer** comparison
(`backend/lowering/expressions.rs:881-883`) — it never compared contents, which
is exactly why `@str_eq` existed. With an aggregate type that same arm calls
`into_int_value()` and breaks.

Reject `==` on arrays and slices in analyze, with a diagnostic pointing at
`str::equals`. Comparing `[N]char` by content inline would hide a loop behind an
operator.

## Work

- [ ] Add the `c"…"` literal to the lexer and a `*char`-typed AST/typed node.
- [ ] Retype string literals as `[N]char`; lower to a non-NUL global.
- [ ] Delete `BuiltinType::String` and its six non-test sites
      (`analyze/expressions.rs:488`, `backend/lowering/types.rs:51` and
      `:163-165`, `backend/lowering/ir_lower.rs:384`).
- [ ] Convert the 54 `@string` externs in `std/libc.fib` to `*char`; fix the 9
      `*@string` declarations.
- [ ] Migrate the 20 `std` modules; rewrite `std/str.fib` around
      `[]char`/`Vec[char]`.
- [ ] Add `Vec[T]` to `std` and `Vec[char]` helpers, including an explicit
      NUL-terminating conversion for runtime-built strings crossing into C.
- [ ] Reject `==` on arrays and slices with a directed diagnostic.
- [ ] Convert every corpus format string to `c"…"`.
- [ ] New `docs/strings.md`; update `docs/literals.md` and `docs/types.md`.

## Tests

- [ ] `"x"` is `[1]char`; `c"x"` is `*char` and the global carries the NUL.
- [ ] `printf(c"%d\n", n)` compiles, links, and runs.
- [ ] A `[]char` view over a `[N]char`.
- [ ] `==` on arrays produces the directed error.
- [ ] An enum payload holding `[]char` sizes correctly.
- [ ] `samples/strings.fib` with a `#[test]` in `tests/e2e.rs`.

## Notes

`.len` starts working on strings, which is an error today.

This is the largest and riskiest document in the series: it touches the C ABI
boundary, the layout of every tagged union that stores text, and every sample.
