# 07 — Interfaces, `self`, and generic constraints

**Status:** proposal. **Prerequisites:** [generic types](04-generic-types.md),
[generic functions](02-generic-functions.md), [overloading](06-function-overloading.md).

## Baseline

There is no implemented interface declaration, trailing `impl` clause, `self`
type, conformance engine or generic constraint checker. The tokenizer/parser
work independently of these future changes. Parsing member calls does not
implement method lookup or static/dynamic dispatch.

## Proposed surface (not accepted grammar today)

```fib
type Ord interface {
    fn compare(a: self, b: self) int32;
    fn default() self;
}

type Dog struct { legs: uint } impl Ord

fn make[T: Ord]() T { return default(); }
```

Conformance is satisfied by free functions; `self` denotes the implementing
type in parameters and results. The historical alternative `impl Interface for
Type` is not current syntax either. Trailing `impl` is a proposal, not a shipped
replacement. Receiver syntax and dynamic interface objects remain deferred.

## Design and implementation work

- [ ] Specify/reserve `interface`, `impl`, `self`; audit identifier collisions.
      Use a dedicated self-type syntax node and reject it outside valid contexts.
- [ ] Share function-signature parsing with interface requirements, requiring
      semicolons and forbidding bodies. Preserve binding identities and spans.
- [ ] Share generic parameter/constraint nodes; propose `T: Ref + Ref`, with
      qualified/generic interface references and clear malformed-list errors.
- [ ] Keep interfaces out of runtime value positions. Add semantic rejection
      and layout/backend backstops, not a guessed vtable representation.
- [ ] Collect claims before use, then verify exact canonical signatures after
      declarations/types are known. Conversion is not conformance matching.
- [ ] Use a bounded worklist if verification instantiates further obligations;
      claims alone never certify success or bypass completion validation.
- [ ] Store conformance by canonical semantic identities, not bare/mangled
      strings or copied import aliases. Resolve functions in specified defining
      environments and enforce visibility consistently across files.
- [ ] Diagnose missing, wrong-signature and ambiguous requirements with source
      context; include instantiation traces for failures inside generic bodies.

## Generic body checking: retained tradeoff, not existing behavior

The old plan chose checking each instantiated body rather than abstract checking.
There is no working template engine today. If adopted, this model permits a body
to depend on concrete-type operations not declared by its constraint: two types
that both claim `Ord` may instantiate differently. Document that limitation
explicitly; constraints are not a Rust-style proof of abstract body validity.

Abstract checking requires parameter/obligation representations and overload
resolution against requirements. Decide this strategy before relying on it for
library safety. A later tightening may preserve syntax but still reject formerly
accepted programs, so it is not automatically source compatible.

## Acceptance tests and deferrals

Test syntax and keyword collisions first. Semantic coverage must include later
function definitions, self in arguments/results, generic type obligations,
canonical cross-file aliases, missing/mismatched/ambiguous functions, non-interface
claims, illegal interface value positions and recursion/resource limits. Backend
backstops and link/run tests come only after those stages exist.

Deferred: receivers/method-call semantics, dynamic dispatch/vtables, inheritance,
default bodies, blanket implementations, associated types/constants. Historical
`Ty` side tables and removed e2e files are not implementation scaffolding.
