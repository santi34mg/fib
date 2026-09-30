# 02 — Generic Functions With `[T]`

**Depends on:** [01](01-builtin-sigil.md). **Blocks:**
[03](03-prefix-array-syntax.md), [04](04-generic-types.md).

## Goal

Replace the "types as positional comptime values" model with a type-parameter
list. Call sites are **always explicit**.

```fib
fn insertion_sort[T](arr: *T, len: int4) void { ... }

insertion_sort[int4](arr.& as *int4, 8);
```

`T: type` parameters are removed from the language.

## Parsing

- Declaration: after the function name, an optional `[` ident (`,` ident)* `]`
  (`parser/function_declaration.rs:16-123`). Add `type_params` to
  `FunctionSignature`.
- The element type is a new `ast/type_param.rs`:
  `TypeParam { name, constraints, span }`, with `constraints` always empty
  until [07](07-interfaces.md). **Share it with `TypeDeclaration` in
  [04](04-generic-types.md)** so constraints land in one place later.
- Call site: `parse_postfix` (`parser/access.rs:17-32`) only consumes `(` and
  `.` today — indexing is `.[i]` and slicing is `.[a..b]` — so a postfix `[` is
  **free**. Add a branch that reads a type-argument list.

## The deletion that unblocks 03

Remove `TypeExpressionKind::TypeKeyword`, `Ty::Type`,
`TypedExprKind::ComptimeType`, and — the load-bearing one — **the builtin-type
branch in expression position** (`parser/primary.rs:113-119`), along with the
backstops at `analyze/types.rs:51`, `analyze/functions.rs:41-56` and `:73-86`,
`ir/expressions.rs:159`, `ir/statements.rs:95,468,505`,
`backend/lowering/expressions.rs:1312`, `backend/lowering/types.rs:98,234`, and
the `type X type` special case at `analyze.rs:240-251`.

After this, **a type can no longer appear in expression position**. That is
precisely the precondition that makes prefix array syntax unambiguous in
[03](03-prefix-array-syntax.md).

The new model is strictly simpler than the one it replaces.

## Analyze

- [ ] `GenericFunctionTemplate.comptime_params: Vec<usize>` becomes
      `type_params: Vec<Identifier>`.
- [ ] `is_generic_function` (`analyze/generics.rs:19-25`) tests
      `signature.type_params` instead of scanning for `TypeKeyword`.
- [ ] `instantiate_generic` (`analyze/generics.rs:256-367`) builds `subs` from
      the explicit type arguments and stops stripping parameters from the
      signature. Keep the mangling, the cache, the signature-only placeholder
      that breaks recursion (L343-356), and the name-sorted drain
      (`analyze.rs:258-264`).
- [ ] Validate type-argument arity, which nothing checks today (roadmap
      L152-154).

## Defects to fix here

- [ ] `substitute_in_expr` (`analyze/generics.rs:128-171`) silently skips
      `Slice { object, start, end }`, `BuiltinCall { args }`, and
      `EnumVariantConstruct { fields }` via a `_ => {}` arm. **Replace the
      catch-alls with exhaustive matches and explicit no-op arms** — the
      catch-all is the root cause, and exhaustiveness makes the compiler find
      the next gap instead of swallowing it.
- [ ] Cross-module generic calls are impossible today:
      `analyze/expressions.rs:826-847` only accepts `TypedSymbol::Function` for
      a qualified callee, so an imported template falls into "not a function"
      even though templates are exported. Fix it, and qualify the mangled name
      with the defining module (roadmap L377).

Forwarding a type parameter to another generic (`other[T](x)`) starts working on
its own: `T` now lives in a *type* position that `substitute_type` already
walks. Today it is impossible, because inside the body `T` is an
`ExpressionKind::Identifier` that nothing rewrites.

## Tests

- [ ] Parser: `f[T]` declares; `f[int4](x)` calls; wrong arity errors.
- [ ] One instantiation per distinct type argument; a repeat hits the cache.
- [ ] `other[T](x)` forwarding inside a template body.
- [ ] A generic called across a module boundary.
- [ ] Update `samples/sorting.fib` — the only sample using generics.

## Docs

- [ ] Rewrite `docs/generics.md`. It documents the old positional form, and its
      example still uses a `for` loop — loops are `while` now, and the docs lag
      behind. Write the new examples with `while`.
