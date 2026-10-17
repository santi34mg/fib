# 06 — Function overloading

**Status:** proposal. **Prerequisite:**
[canonical declaration/linkage identity](00-cross-module-linkage.md).
**Related:** [interfaces](07-interfaces.md).

## Baseline and historical choices

Parsing functions is implemented; overload collection, type checking,
coercions, expected-type propagation and symbol emission are not. Current lexical
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
  Do not propagate it into cast operands unless deliberately specified.
- With no usable expected type, ambiguity is an error, never first-match choice.

## Proposed algorithm

Gather candidates; filter generic arity and runtime arity; type arguments once
using a contextual parameter type only when viable candidates agree; classify
coercions without mutating syntax; use return context and per-argument dominance
to select a unique candidate; apply conversions only after selection.

Exact matches should beat permitted widening. Do not sum ranks or silently
allow signedness/family conversions. Define contextual literal behavior before
claiming that an integer literal necessarily chooses one integer width.

## Work and tests

- [ ] Specify overload-set storage and redeclaration rules in project symbols,
      separately from lexical binding scopes and member resolution.
- [ ] Share one pure conversion classifier with actual coercion insertion.
- [ ] Ensure speculative candidates cannot emit instances or poison caches.
- [ ] Diagnose candidates with signatures and locations, including return-only
      ambiguity in statement position and overloaded names used as values.
- [ ] Test every expected-type site, exact/widening choice, incomparable rank
      vectors, aliases, generics, variadics and incompatible foreign signatures.
- [ ] Add deterministic cross-file link/run tests after pipeline implementation.

Typing arguments once can leave `outer(inner())` ambiguous when both are
overloaded. Document this bounded policy or design constraint solving explicitly;
do not promise arbitrary nested inference. Old call-site counts and end-to-end
tests do not exist as a current implementation safety net.
