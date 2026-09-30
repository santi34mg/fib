# 03 — Prefix Array and Slice Syntax

**Depends on:** [02](02-generic-functions.md). **Blocks:**
[04](04-generic-types.md).

Supersedes the parser-hardening item "migrate slices from `T[]` to `[]T` and
arrays from `T[N]` to `[N]T`" in the Phase 4 checklist.

## Goal

`@int4[8]` becomes `[8]int4`. `@int4[]` becomes `[]int4`. An array of two
arrays of three is `[2][3]int4`, read outside-in.

Value syntax is untouched: indexing stays `arr.[i]`, slicing stays
`arr.[a..b]`, array literals stay `[1, 2, 3]`.

## Why this comes after 02

In **type** position there is no new ambiguity. All ten entry points into
`parse_type_expression` are reached after a token that disambiguates (`:`,
`as`, `type`, `*`, `(`, `->`), and `[` starts nothing else there.

The one new ambiguity would be in **expression** position, between an array
literal `[1,2,3]` (`parser/primary.rs:77-97`) and a type value `[]int4`.
Separating them needs a scan to the matching `]`. **[02](02-generic-functions.md)
removes it at the root** by deleting types from expression position: after that
PR, a `[` in an expression is always an array literal.

## Implementation

One site: `parser/type_expression.rs:85-128` flips from suffix to prefix.

The suffix form was a left-recursive `if`. The prefix form is **right
recursion** — `parse_type_expression` calls itself after `[N]` — which is
strictly simpler and makes nesting work for free.

`@int4[2][3]` does **not** parse today (the suffix is an `if`, not a loop), so
nested arrays are new functionality and need a sample. The AST already supports
them (`element_type: Box<TypeExpression>`), and `map_type`, `map_type_to_llvm`,
`substitute_type`, and `mangle_type_expr` already recurse correctly.

Additional win: `*[8]int4` (pointer to array) and `[8]*int4` (array of
pointers) become distinguishable without parentheses. Today `*@int4[8]` is only
pointer-to-array because of an ordering subtlety between `parser/pointer.rs:14`
and the suffix block — easy to misread.

## Scope

The corpus is tiny: **7 occurrences in 3 files** —
`samples/slices.fib` (lines 1, 7, 18, 19), `samples/sorting.fib` (lines 1, 33),
`std/io/stream.fib:32`. Nothing complicated exists anywhere: no pointer-to-array,
no array-of-pointers, no slice-of-struct, no multidimensional.

## Work

- [ ] Flip `parser/type_expression.rs:85-128` to prefix right recursion.
- [ ] Update both `Display` impls: `ast/type_expression.rs:82-87` and
      `typed_ast.rs:201-202`.
- [ ] Update the single printed-form assertion,
      `typed_ast/test.rs:134-138`.
- [ ] Update the ~45 source strings in `analyze/test.rs`, plus
      `parser/test.rs:811,828`, `backend/lowering/test.rs:523-572`,
      `tests/e2e.rs:191`.
- [ ] Update the 7 corpus occurrences.
- [ ] Update the doc comments that spell the old syntax
      (`ast/type_expression.rs:47`, `ast/expression.rs:65`,
      `typed_ast.rs:175,362,368`, `analyze/expressions.rs:286,315,317`,
      `parser/type_expression.rs:85,94`, `backend/lowering/statements.rs:205`).
- [ ] Docs: `docs/arrays.md` (13 sites), `docs/types.md:26-27`,
      `docs/builtins.md:22-23`, `docs/generics.md:28`, `docs/operators.md:41`,
      `samples/README.md:28-29`.
- [ ] Tick the superseded parser-hardening item in the Phase 4 checklist.

The LSP needs **no change**: `lsp/src/highlight.js` has no type grammar and
treats `[` as plain punctuation.

## Tests

- [ ] `[8]int4`, `[]int4`, `[2][3]int4` parse into the right shapes.
- [ ] `*[8]int4` and `[8]*int4` parse differently.
- [ ] `[1,2,3]` in expression position is still an array literal.
- [ ] `samples/nested_arrays.fib` with a `#[test]` in `tests/e2e.rs` — nested
      arrays are a new lowering path.
