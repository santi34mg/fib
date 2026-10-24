# 06 — Function overloading

**Status:** proposal. **Prerequisite:**
[canonical declaration/linkage identity](00-cross-module-linkage.md).
**Related:** [interfaces](07-interfaces.md).

## Baseline and historical choices

Ordinary function parsing, semantic typing, contextual literal checking, metatype
specialization and internal symbol emission work. Overload sets, overload
selection and overload-specific conversion ranking remain unimplemented. Current lexical
scope utilities are not the old `Vec<TypedSymbol>` overload engine. Removed
`TypedModule` exports, `coerce_to`, `dedupe_declarations` and separate typed AST
must not dictate new APIs or backend symbol lookup.

## Proposed semantics

Allow overloads by parameter types and, when context disambiguates, return type.
Return-type overloads support free interface functions such as `default()`.
This remains a design choice, not a supported language feature.

- Collect the entire declaration set before checking bodies. Separate forward
  visibility from definite initialization as required by
  [name resolution](../name-resolution.md).
- Use canonical semantic types for duplicate detection and canonical declaration
  identity for linkage. Mangling written aliases can disagree with type equality;
  settle the encoding rather than repeating that historical recommendation.
- Give foreign symbols and the entry point explicit non-overload/linkage policy.
  One entity reached by two aliases is not a second overload.
- Pass expected type explicitly, not as sticky global context. Proposed sites:
  annotated bindings, returns, assignments, call arguments and aggregate fields.
  Preserve the confirmed direct same-family literal cast context; do not
  propagate a cast target into typed operands or arithmetic expressions.
- With no usable expected type, ambiguity is an error, never first-match choice.

## Proposed algorithm

Gather candidates; filter ordinary argument arity and metatype argument kinds; type arguments once
using a contextual parameter type only when viable candidates agree; classify
compatibility without mutating syntax; use return context and per-argument
dominance to select a unique candidate. Preserve explicit numeric conversions.

The confirmed numeric policy forbids implicit widening and signedness/family
conversions between typed numbers. Overloading must preserve it. Rank exact
compatibility and any permitted readonly-view conversions without summing ranks;
specify contextual literal disambiguation before claiming that a literal chooses
one integer width. Numeric conversions require explicit casts.

## Work and tests

- [ ] Specify overload-set storage and redeclaration rules in project symbols,
      separately from lexical binding scopes and member resolution.
- [ ] Share one pure compatibility classifier with any permitted view-conversion insertion.
- [ ] Ensure speculative candidates cannot emit instances or poison caches.
- [ ] Diagnose candidates with signatures and locations, including return-only
      ambiguity in statement position and overloaded names used as values.
- [ ] Test every expected-type site, exact/readonly-view choice, incomparable rank
      vectors, aliases, metatype specializations and incompatible foreign signatures. Variadics
      are removed and are not part of this proposal.
- [ ] Add deterministic cross-file link/run tests when cross-file lowering is implemented, using the existing harness.

Typing arguments once can leave `outer(inner())` ambiguous when both are
overloaded. Document this bounded policy or design constraint solving explicitly;
do not promise arbitrary nested inference. Existing ordinary-call and executable tests do not establish overload support.
