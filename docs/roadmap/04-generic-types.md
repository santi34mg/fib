# 04 — Generic data types

**Status:** proposal. **Depends on:** explicit generic syntax and semantic
identity; see [02](02-generic-functions.md) and [03](03-prefix-array-syntax.md).
**Related:** [strings](05-remove-string.md), [interfaces](07-interfaces.md).

## Baseline

Type declarations currently parse `type Name` followed by a type expression;
there is no parsed generic parameter list or generic application type node.
Semantic type records exist, but template resolution/instantiation and layout
validation do not run. Removed `Ty`, `TypedSymbol`, `TypedDecl` and global-frame
insertion APIs are not current implementation targets.

## Proposed surface

```fib
type Vector[T] struct {
    ptr: *T,
    len: usize,
    cap: usize,
}
```

Proposed use sites are `Vector[int32]`, nested instances and explicit generic
constructors. These are future syntax. Specify constructor/binding grammar
separately; do not carry forward the old `v: Type = value` example as current
syntax. Prefix arrays would remove suffix-type ambiguity, but value indexing
still requires the bracket decision in [02](02-generic-functions.md).

## Identity and representation

The historical proposal encoded nominal types as mangled `Ty::Identifier`
strings to reuse old structural-equality and LLVM helpers. That rationale no
longer applies. Use canonical semantic declaration/instance identity with
project-owned `TypeId`s, then map to IR-owned runtime types. Linker/diagnostic
names must not define type equality, and the backend must not search lexical
scopes to resolve a mangled source name.

## Work

- [ ] Share spanned, binding-bearing parameters with functions; specify arity,
      constraints, qualified applications, bare-template errors and constructors.
- [ ] Resolve templates in their defining environment and preserve visibility
      and canonical identity across aliases and repeated references.
- [ ] Cache by declaration and canonical arguments, register in-progress
      identities before recursive expansion, and emit deterministically.
- [ ] Substitute exhaustively through fields, enum payloads, expressions and
      nested instances; diagnose unresolved names before backend handoff.
- [ ] Detect infinite by-value layout separately from recursive instantiation
      growth. A pointer/slice can break a layout cycle but does not necessarily
      bound expansion of distinct instances such as `Wrap[Wrap[T]]`.
- [ ] Specify alias-cycle diagnostics and a resource/depth limit for both data
      and function instantiation. A simple “any indirect edge” flag is not a
      substitute for a correct dependency/layout cycle analysis.

## Acceptance tests

Distinct arguments produce distinct nominal instances; repeats reuse one;
aliases reach the same instance. Cover nested structs/enums, indirect recursion,
infinite value layout, expanding recursion, wrong arity, alias cycles and
cross-file references to private defining-environment helpers. Once lowering
works, validate layout and emitted identity with executable tests.

Inference, defaulted/higher-kinded parameters and constraints beyond
[07](07-interfaces.md) remain deferred. Do not revive consumer-scope flattening
or spelling-based instance deduplication to reuse historical code.
