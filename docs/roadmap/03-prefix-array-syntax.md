# 03 — Prefix array and slice types

**Status:** future syntax change. **Related:**
[generic functions](02-generic-functions.md), [generic types](04-generic-types.md).

## Actual baseline

`parser/type_expression.rs` parses a type followed by one optional array/slice
suffix: `int32[8]`, `int32[]`. That suffix block is not a general suffix loop;
do not infer arbitrary multidimensional suffix support from recursive AST data.
Values already use `arr[i]`, `arr[a..b]` and `[1, 2, 3]`. Historical `arr.[i]`
and `@int4[8]` are not syntax to preserve.

## Proposal

Use `[8]int32`, `[]int32` and `[2][3]int32`, read outside-in. Distinguish
`*[8]int32` (pointer to array) from `[8]*int32` (array of pointers). Leave
current value indexing, slicing and array literals unchanged.

Prefix parsing can recurse after `[N]` or `[]`. Do not claim existing type
mapping or LLVM recursion makes this work end to end: those passes are stubs.

## Decisions and work

- [ ] Specify array-size grammar/ranges and precedence with pointers, tuples,
      function types and proposed generic applications.
- [ ] Resolve expression-position ambiguity with array literals if prefix types
      can become type values. The earlier proposal removes type values via
      [02](02-generic-functions.md), but that policy is not implemented or an
      established dependency on working generic functions.
- [ ] Update parser and AST formatting together; decide whether old suffixes
      are rejected with migration diagnostics or temporarily accepted.
- [ ] Audit current fixtures, std/samples, documentation and highlighting when
      implementing the migration. Historical occurrence counts are obsolete.
- [ ] Specify finite layout and lowering for nested arrays independently of
      parser acceptance; use the owned SSA route only.

## Acceptance tests

Positive/negative parser tests: prefix arrays/slices, multidimensional shape,
pointer/array precedence, qualified/generic elements, literal disambiguation,
missing delimiters, invalid sizes and old-syntax policy. Add semantic/layout
and execution tests as those stages become available; there is no e2e harness
or active backend array lowering to rely on today.
