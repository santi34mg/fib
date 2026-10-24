# 07 — Interfaces, `self`, and generic constraints

**Status:** proposal. **Related:** implemented [metatype specialization](../type-resolution.md)
and proposed [overloading](06-function-overloading.md).

## Baseline

There is no implemented interface declaration, trailing `impl` clause, `self`
type, conformance engine or generic constraint checker. The tokenizer/parser
work independently of these future changes. Parsing member calls does not
implement method lookup or static/dynamic dispatch.

## Proposed surface (not accepted grammar today)

The proposed capability is an interface describing free-function requirements,
including comparison and default construction. `self` would denote the implementing
type in parameters and results. Interface declarations, conformance claims and
constraints on ordinary `T: type` parameters still need syntax decisions.
Type declarations use `comp` bindings.

Trailing `impl` and the historical `impl Interface for Type` are unimplemented
alternatives. Receiver syntax and dynamic interface objects remain deferred.

## Design and implementation work

- [ ] Specify/reserve `interface`, `impl`, `self`; audit identifier collisions.
      Use a dedicated self-type syntax node and reject it outside valid contexts.
- [ ] Share function-signature parsing with interface requirements, requiring
      semicolons and forbidding bodies. Preserve binding identities and spans.
- [ ] Define constraint syntax and semantic obligations for ordinary metatype
      parameters, including qualified interface references and malformed-input tests.
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

## Generic body checking and constraints

Existing metatype specialization combines symbolic template checks with concrete
body rechecking. Interface obligations must integrate with that mechanism. Decide
whether a constrained body may use concrete operations absent from its requirements;
conformance alone must not be presented as proof of abstract body validity.

Abstract checking requires parameter/obligation representations and overload
resolution against requirements. Decide this strategy before relying on it for
library safety. A later tightening may preserve syntax but still reject formerly
accepted programs, so it is not automatically source compatible.

## Acceptance tests and deferrals

Test syntax and keyword collisions first. Semantic coverage must include later
function definitions, self in arguments/results, generic type obligations,
canonical cross-file aliases, missing/mismatched/ambiguous functions, non-interface
claims, illegal interface value positions and recursion/resource limits. Backend
backstops and tests through the existing executable harness follow interface
implementation and runtime specialization lowering.

Deferred: receivers/method-call semantics, dynamic dispatch/vtables, inheritance,
default bodies, blanket implementations, associated types/constants. Historical
`Ty` side tables and removed e2e files are not implementation scaffolding.
