# 07 — Interfaces, `self`, and Generic Constraints

**Depends on:** [04](04-generic-types.md), [06](06-function-overloading.md).
Implements Phase 9 "Add Interfaces and Static Dispatch".

Supersedes the Phase 9 assumption of `impl Interface for Type` syntax.

## Goal

```fib
type Ord interface {
    fn compare(a: self, b: self) int4;
    fn default() self;
}

type Dog struct {
    legs: uint
} impl Ord

fn compare(a: Dog, b: Dog) int4 { ... }
fn default() Dog { ... }

fn make[T: Ord]() T { return default(); }
```

`self` is the implementing type and is valid in **parameters and return type**.
Conformance is satisfied by **free functions**, found through
[06](06-function-overloading.md).

That `self` works in parameter position matters: most methods are then
distinguished by their parameters, and return-type dispatch is left for the
degenerate `fn default() self`.

## Syntax

- [ ] Add `Keyword::Interface`, `Keyword::Impl`, `Keyword::SelfType` plus lexer
      entries (`lexer.rs:550-584`). All three are free in the corpus;
      `Keyword::Union` — declared and never consumed — is the precedent that
      reserving a keyword is cheap. Grep the in-memory `.fib` strings in
      `src/**/test.rs` before landing.
- [ ] Fifth arm in the type-kind match (`parser/type_expression.rs:27-36`):
      `TypeExpressionKind::Interface { methods: Vec<FunctionSignature> }`.
      **Reuse `FunctionSignature` verbatim** — an interface method is exactly
      that. Extract `parse_function_signature()` out of
      `parse_function_declaration` so both share one path; the interface form
      requires a trailing `;` and forbids a body.
- [ ] Trailing `impl Ref (, Ref)*` on a type declaration. No ambiguity: `impl`
      is a new keyword and does not start a top-level declaration
      (`parser.rs:204-243`). Each `Ref` is an `Identifier`,
      `QualifiedIdentifier`, or `GenericInstance`, so `impl Container[int4]`
      comes free from [04](04-generic-types.md). Reject a second `impl` clause
      with "use a comma to list multiple interfaces".

### `self`

Use a dedicated `TypeExpressionKind::SelfType`, not a reserved identifier. A
reserved identifier would silently turn a user's field named `self` into a type
and would make `self` outside an interface a confusing "unknown type" error.

Substitute it through the reserved key `"self"` in the existing
`HashMap<String, TypeExpression>`. Because `self` is a keyword, no type
parameter can be named `self`, so the key is unforgeable — and this avoids
adding a parameter to `substitute_type` and its 10+ call sites.

## Interfaces are not value types

- [ ] Add `Pos::InterfaceDecl` to the position parameter from
      [04](04-generic-types.md) for the direct body of `type X interface`.
      Everywhere else, a type resolving to `Ty::Interface` is an error with a
      hint pointing at generic parameters. One site instead of five (parameter,
      return, field, local, cast target).
- [ ] Backstop anyway: give `typed_type_size_align` and `map_type_to_llvm` a
      `Ty::Interface` arm returning a layout error, so a missed path fails
      cleanly instead of miscompiling.

Dynamic dispatch and vtables stay deferred, as Phase 9 already states.

## Conformance: claim, then verify

An obligation cannot be verified until **all** functions are registered, but
generic constraint checks fire *during* the main pass, at call sites. Split it:

- [ ] **Claims pre-pass** (~15 lines, before the main loop): walk the
      declarations and record "`Dog` claims `Ord`". Names only, no type
      resolution. This removes the use-before-declaration trap for constraints.
- [ ] **Verification** after the main loop and after the generic drains, as a
      **worklist to fixpoint** — verifying an obligation can `map_type` an
      interface reference that triggers an instantiation that pushes another
      obligation. Bound it with the same depth cap.

Constraint checks consult **claims**. The claim is trusted; verification
guarantees it or the compile fails. This is the subtle part and deserves a
comment in the source.

Signature matching is **exact after `resolve_type_alias` on every position** —
no `coerce_to`, no variance. Conformance is not the place for implicit
conversion.

Diagnostics: missing method, present but wrong signature, ambiguous match.

## Conformance storage

- [ ] A **side table** on `SymbolTable`:
      `conformances: HashMap<String, BTreeSet<String>>`, `BTreeSet` for
      deterministic diagnostics. This mirrors the existing `modules` field,
      which already lives at table level.

Rejected: a field on `TypedSymbol::Type`, which would cost 15 pattern-match
sites across 10 files and reach into the backend.

- [ ] One `conformance_key(ty, scope)` used at both claim and query time, so
      `Ty::Identifier("Dog")` and `Ty::QualifiedIdentifier { animals, Dog }`
      canonicalize to the same key.
- [ ] `TypedModule` carries its map; the import loop merges each imported
      module's entries. **Selective imports must also merge under the bare
      local name**, because a selectively imported `Dog` is referenced as
      `Ty::Identifier("Dog")` and otherwise loses its origin.

This is the fiddliest part of the document and wants a driver test with a
three-module diamond. It reads exports, so keep it behind the
`export`/`export_all` helpers from [06](06-function-overloading.md) to stay out
of Phase 3's way.

## Generic constraints

- [ ] `TypeParam.constraints`, parsed as `T : Ref ('+' Ref)*`. Support `+` from
      the start — three lines — rather than breaking the syntax later. Delete
      the "not supported yet" error from [04](04-generic-types.md).
- [ ] Check in `instantiate_generic` **before** substitution, so the error names
      the call site with the parameter still abstract:
      "`Cat` does not implement `Ord`, required by type parameter `T` of
      `make`".

### Bodies are checked per instantiation, not abstractly

This is already the design: `analyze.rs:225-229` returns `None` for generic
functions — templates are never analyzed — and `instantiate_generic` clones,
substitutes, and re-analyzes. Constraints slot in with no new machinery.

The abstract alternative needs a `Ty::Param` variant, an overload resolver that
dispatches against an interface's method table, and — decisively — `coerce_to`,
the *only* assignability rule and built on structural `Ty` equality, would have
to grow abstract-type cases. That is the "audit every `==`" hazard, and a
separate project.

Return-type and parameter-type resolution then come free: after substitution the
body reads `x: Dog = default()`, which [06](06-function-overloading.md) resolves
with no new code.

**The honest cost, to document in `docs/generics.md`:** the constraint is *not*
enforced inside the body. `fn make[T: Ord]() T` may call anything that happens
to exist for the concrete `T`, so it can compile for `T = Dog` while relying on
Dog-specific functions and fail for `T = Cat` even though `Cat impl Ord`. This
is C++ template semantics plus a `static_assert`, not Rust trait semantics.

Tightening to abstract checking later needs **no source-syntax change** — only
the checking strategy hardens. That exit path is why this choice is safe now.

- [ ] Append "while instantiating `make[Dog]` (line N)" to
      `AnalysisError::hint` from the `in_progress` stack, so template-body
      errors surfacing at the instantiation stay legible.

## Tests

- [ ] Conformance where the function is defined **after** the type (proves the
      second pass).
- [ ] `self` in parameter position: `fn compare(a: self, b: self) int4`.
- [ ] Generic type conformance verified per instantiation
      (`type Vector[T] … impl Container[T]`).
- [ ] Cross-module conformance in both aliased and selective import spellings.
- [ ] Negatives: missing method; wrong return type; ambiguous match;
      `impl NotAnInterface`; an interface in each of the four value positions;
      `self` outside an interface; `make[Cat]()` where `Cat` lacks the `impl`.
- [ ] Backend unit test: an interface leaking into a struct field hits the
      layout backstop.
- [ ] `samples/interfaces.fib` with a `#[test]` in `tests/e2e.rs`.
- [ ] New `docs/interfaces.md`; tick Phase 9 items 1, 2, 4, 5, 6, 9.

## Deferred

Receiver syntax and method calls — `obj.foo()` parses
(`parser/access.rs:17-32`) but analyze rejects it at
`analyze/expressions.rs:872-878`; conformance is by free functions and that
error stays. Also: dynamic interface objects, interface inheritance, default
method bodies, blanket impls, associated types and consts.
