# 01 — Builtin spelling and possible function removal

## Implemented baseline

The tokenizer recognizes bare bit-width types (`int32`, `uint64`, `float32`)
and default aliases `int`/`uint` = 32 bits. `.len` parses as ordinary member
selection. Legacy `@` types and byte-width `int4` are not current source syntax.
Do not repeat the historical bulk rename as an outstanding compiler feature.
Some `Display` implementations and comments still print legacy sigils; their
output is not a normative grammar or round-trip guarantee.

Builtin functions remain: `@str_len`, `@str_eq`, `@concat` parse builtin calls.
The bare-name tokenizer branch also recognizes those names; audit reservation
and diagnostics before specifying a stricter surface. Analysis and runtime
behavior are unimplemented. `@` still tokenizes; it has not been removed from
the language or adopted as a working attribute system.

## Retained proposal (not implemented)

Replace these functions with ordinary library operations, potentially alongside
[the string redesign](05-remove-string.md). This is not a consequence of bare
type spelling and is not required before every generic parser change.

- [ ] Specify replacement APIs, allocation/failure/ownership behavior and a
      migration path before removing builtin call syntax/AST forms.
- [ ] Specify array/slice `.len` semantics and result type. Member selection
      must depend on the resolved receiver type: a struct field named `len`
      remains a field, not a magic name recognized before typing.
- [ ] Audit actual std/sample syntax and module-name collisions; old counts,
      the proposed `std/string.fib` rename and old e2e outputs are historical
      migration notes, not verified facts about supported code.
- [ ] Reconcile formatting, diagnostics and editor highlighting with accepted
      syntax, without using inactive backend code as an implementation route.

## Acceptance tests

Test bare type tokens, aliases, obsolete spellings, `.len` member syntax and
malformed builtin calls now. Add semantic tests for struct `len` versus array
and slice length only when typing exists; add library/runtime equivalence tests
only when the execution pipeline works. See the [baseline](README.md).
