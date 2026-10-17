# 02 — Explicit generic functions with `[T]`

**Status:** proposal. **Related:** [prefix arrays](03-prefix-array-syntax.md),
[generic types](04-generic-types.md), [constraints](07-interfaces.md).

## Current implementation

Function parsing expects `(` immediately after the name. The AST signature has
an always-empty reserved `generic_parameters` string list; it is not working
generic declaration syntax or a binding-bearing type-parameter model. Builtin
types still parse as expression type values, and `type` remains a builtin
metatype used by type-declaration dispatch. Analysis, substitution,
instantiation, caching and code generation are unimplemented.

## Proposed syntax

```fib
fn identity[T](x: T) T { return x; }
y := identity[int32](1);
```

These examples are future grammar, not runnable Fib today. Require explicit
arguments and share binding-bearing `TypeParam` nodes with generic data types,
including source spans and later constraints.

### Grammar decisions required first

Postfix `[` is **not free**: `a[i]` and `a[a..b]` already parse. `f[T](x)`
can look like calling an indexed value. Specify syntactic disambiguation or an
unresolved bracket node plus semantic resolution, with malformed-input tests.
Prefix arrays do not by themselves remove this value-position ambiguity.

The old series proposed deleting type values and positional `T: type` generics.
That is a future language decision, not a deletion of a working generic engine.
If adopted, preserve metatype/type-declaration parsing deliberately and reconcile
compile-time aliases with [name resolution](../name-resolution.md).

## Semantic design and work

- [ ] Collect templates by defining declaration identity; resolve names in the
      defining environment, not by copying exports into consumer lexical scopes.
- [ ] Check explicit argument arity/kinds and runtime call arity separately.
- [ ] Key instances by declaration plus canonical semantic type arguments; use
      deterministic emission and canonical linkage, not display-string identity.
- [ ] Use an in-progress instance state to permit ordinary recursion and a
      bounded resource/depth policy to diagnose expanding polymorphic recursion.
- [ ] Substitute exhaustively through every type/expression/statement shape,
      including slices, builtin arguments, enum payloads and nested instances.
- [ ] Annotate instantiated source syntax under a documented ownership strategy,
      then lower through `AnalyzedProject -> Ir`; no separate typed AST fallback.

## Acceptance tests

Parser success/error coverage must include generic declarations, bracket/index
ambiguity, nested arguments and truncated lists. Later semantic tests cover
arity, repeated cache hits, `other[T](x)` forwarding, recursion limits and
cross-file templates with multiple aliases. Execution tests require a new
harness; no historical sorting sample or removed analyzer test proves support.
