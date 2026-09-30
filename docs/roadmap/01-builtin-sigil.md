# 01 — Remove the `@` Sigil From Builtins

**Depends on:** nothing. **Blocks:** [02 — Generic Functions](02-generic-functions.md).

## Goal

Built-in **types** are spelled `int4`, not `@int4`. The comptime property is
`.len`, not `.@len`. The three built-in **functions** are deleted outright.

After this, `@` is no longer a language sigil. `Punctuation::At` stays reserved
for future attribute syntax, which is what `src/frontend/tokens/punctuation.rs`
already says it is for.

## Why the builtin functions go away

`@str_len`, `@str_eq`, and `@concat` are all implementable in Fib, so they do
not need to be in the compiler:

- `@str_len` calls `strlen`. Once [05](05-remove-string.md) lands, length lives
  in the type or in the slice header, so this becomes `.len`.
- `@str_eq` is `strcmp(a, b) == 0`. A comparison loop is trivial in Fib.
- `@concat` is `malloc` + two `strlen` + two `memcpy`
  (`src/backend/lowering/expressions.rs:954-1031`). `std/string.fib:20` only
  wraps it today, but the pattern it needs — `libc::malloc`, `strcpy`, and a
  `*char` walk — is already written twice in that same file, in `to_upper` and
  `to_lower`.

All three also depend structurally on NUL termination, which is exactly what
[05](05-remove-string.md) removes. `@concat` additionally leaks by design.

`.len` **stays** a builtin: on an array it is a comptime constant, on a slice it
is a load of field 1 of the `{ptr, i64}` header. It is a layout query, not a
function.

## `.len` versus a field named `len`

A struct may have a field named `len` — `Vector[T]` in
[04](04-generic-types.md) does. This is not ambiguous: it resolves on the
**type of the object**, which analyze already has. Arrays and slices have no
user fields; structs have no builtin `.len`.

`src/frontend/analyze/expressions.rs:908-923` currently tests the *name* first
(`if field.value == "@len"`) and the type second. Invert that order.

## The `string` module collision

The 20 type names become effectively reserved. A corpus sweep finds none of them
used as an identifier **except `string`**: `std/string.fib` is a module, and
`import std::string` appears in 8 files. Module path segments lex as
`TokenKind::Identifier`, so those imports would stop parsing.

Rename `std/string.fib` to `std/str.fib`. [05](05-remove-string.md) rewrites
that module around `[]char`/`Vec[char]` anyway, so `str` is the end-state name
and there is no churn back.

`concat` stops colliding because the builtin is deleted. `len` as a parameter
name (41 uses) never collided: `.len` is only valid after a `.`.

## Work

### Compiler

- [ ] Drop the `@` from all three `Display` impls in
      `src/frontend/tokens/builtin.rs`.
- [ ] Delete the `BuiltinFunction` enum (`builtin.rs:119-155`) and every
      consumer: `parser/primary.rs:122-139`,
      `analyze/expressions.rs:497-537`,
      `backend/lowering/expressions.rs:954-1031`,
      `ExpressionKind::BuiltinCall`, and `Instruction::BuiltinCall`
      (`ir/mod.rs:134`).
- [ ] Resolve builtin type and property names in `lex_identifier_or_keyword`
      (`lexer.rs:550-584`); delete the now-obsolete comment at L577-578.
- [ ] Make `@` followed by a builtin name a `TokenKind::Error` carrying a
      `with_hint` (`src/diagnostics.rs:106`) that names the new spelling.
- [ ] Rename the `"@len"` magic string to `"len"` on **both** sides at once:
      `parser/access.rs:276` and `analyze/expressions.rs:909`.
- [ ] Invert the name/type test order for `.len` (see above).
- [ ] Fix the hardcoded `@` in the diagnostics at
      `analyze/expressions.rs:523` and `analyze/statements.rs:594`.

### Standard library

- [ ] Rename `std/string.fib` to `std/str.fib`; update the 8 importers.
- [ ] Reimplement `str::concat` with `libc::malloc`/`strlen`/`memcpy`.
- [ ] Add `str::str_len` and `str::str_eq` as ordinary functions.
- [ ] Point `samples/string_builtins.fib` at `std::str`; its pinned stdout in
      `tests/e2e.rs:166` must not change.

### Corpus

- [ ] Rewrite ~1281 `@` tokens across 40 `.fib` files with a `sed` restricted
      to the 20 type names plus `len`, then review by hand.
- [ ] Update the Rust fixtures: `analyze/test.rs` (128), `driver.rs` (36),
      `parser/test.rs` (20), `tests/probe_tmp.rs` (19), `lexer/test.rs`,
      `ir/test.rs`, `backend/lowering/test.rs`, `tests/e2e.rs`.
- [ ] Update the `Display` assertions at
      `src/frontend/typed_ast/test.rs:123-138`.

### Docs and tooling

- [ ] Rewrite the normative claims: `docs/types.md:3-6`, and
      `docs/builtins.md:3-5` — "everything builtin is spelled with a leading
      `@`" stops being true, and that page shrinks to just `.len`.
- [ ] Update `docs/operators.md:71`, `README.md:70-71`, `docs/README.md`, the
      remaining `docs/*.md`, `docs/agents/**`, `samples/README.md`,
      `std/README.md`.
- [ ] Reach `builtinTypes`/`builtinProperties` from the identifier branch
      (`lsp/src/highlight.js:215`) instead of the `@` branch (L200-214); delete
      `builtinFunctions`. **`lsp/editors/vscode/server/highlight.js` is a
      byte-identical copy — move both together.** Update
      `lsp/test/highlight.test.js:27-28`.
- [ ] Add `float2` to the `docs/types.md` table; it exists in the enum but is
      missing from the docs.

## Free side effect

`mangle_type_expr` (`analyze/generics.rs:30`) formats through `Display`, so
generic instantiation symbols go from `insertion_sort__@int4` to
`insertion_sort__int4`. That removes an `@` from inside a symbol that only works
today because inkwell quotes it.

## Tests

- [ ] `@int4` is a lex error whose hint names `int4`.
- [ ] `.len` on a struct with a `len` field resolves to the field; on an array
      and on a slice it resolves to the builtin.
- [ ] The existing corpus is the regression net for the rename itself.
