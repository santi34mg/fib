# 06 — Function Overloading

**Depends on:** [00](00-cross-module-linkage.md). **Blocks:**
[07](07-interfaces.md).

## Goal

Overload resolution on **parameter types and return type**. Interfaces need it:
two types in one module that both implement `Default` each need their own
`fn default()`, distinguished only by what they return.

## Do not touch import resolution

Phase 3 will replace `{ module, member }` with canonical `ModuleId` paths and
rework how modules are imported. This document must not pre-empt that: it reads
the symbol computed in [00](00-cross-module-linkage.md) and never recomputes it,
and it isolates export access behind helpers so Phase 3 can change the shape
underneath.

The mangling in [00](00-cross-module-linkage.md) and here **is** Phase 3's
"Declaration Identity and Linkage" work. Tick those items rather than planning
to redo them.

## Symbol table

`scopes: Vec<HashMap<Identifier, Vec<TypedSymbol>>>`, with the invariant that an
entry holds more than one element only when all of them are functions.

This keeps `lookup(&Identifier) -> Option<&TypedSymbol>` byte-identical at all 32
existing call sites, 12 of which are in the backend and must not change.

Rejected: `TypedSymbol::FunctionSet(..)` adds a variant to an enum the backend
destructures with catch-alls, so an escaped `FunctionSet` would surface as a
confusing runtime error instead of a compile error.

- [ ] Add `lookup_all`, `insert_overload`, `merge_imported`.
- [ ] Add `TypedModule::export` / `export_all` helpers, migrate the call sites
      to them, and **only then** change the `exports` value type. Two steps,
      each compiling.

## Mangling: always, with two exemptions

`extern` (C ABI) and `main` in the entry unit.

"Mangle only when overloaded" is unimplementable: if module `a` defines
`fn f(int4)` and module `b` imports it and adds `fn f(float8)`, `a`'s
already-emitted symbol would have to change retroactively. Mangling always makes
the emitted symbol a pure function of the declaration, which is what `decl_key`,
diamond dedup, and Phase 3 all need.

Subtlety: mangle on the **written** type (short names, readable `.ll`), but
compare **alias-resolved** types for duplicate detection. `type Dog = struct{…}`
makes `fn f(Dog)` and `fn f(struct{…})` the same function with different mangled
names. Both rules are needed; neither suffices alone.

## Expected-type propagation

`AnalyzeCtx` gains `return_type` — `validate_return_types`
(`analyze/functions.rs:110-205`) runs *after* the whole body is typed, so
`return default();` has no other source for it.

But `expected` is an **explicit parameter, not a context field**. As a field it
becomes sticky state that leaks into subexpressions, and `g(f())` would see
`g`'s expected type while typing `f`.

Propagate at: annotated declarations, `return`, assignment, field assignment,
call arguments, struct field initializers, enum payload fields.

**Deliberately not into cast operands.** Propagating would turn `as` into an
overload selector, making `default() as Dog` and `var Dog d = default()` mean
different things while looking identical.

With no expected type available, resolution runs on parameters alone. More than
one survivor is an error — never "pick the first".

## Algorithm

Gather candidates → partition by explicit type arguments → arity filter →
**type the arguments exactly once** (using a parameter type only where all
viable candidates agree on it) → score coercions → filter by return type if
still ambiguous → dominance → apply.

- [ ] Extract `classify_coercion(target, source, kind, scope) -> Option<Rank>`
      from `coerce_to` (`analyze/expressions.rs:292-380`), which consumes a
      `TypedExpr` and builds nodes and so cannot serve as a predicate. Rewrite
      `coerce_to` on top of it so predicate and coercion cannot drift.
- [ ] Split `Widen` (same numeric family and signedness, wider) from `Convert`
      (everything else numeric). Without this split the catch-all at
      `analyze/expressions.rs:369` makes every numeric type coerce to every
      other, so `f(int4)` + `f(float8)` called with an `int8` ties and the user
      gets an ambiguity error for an obvious call. **This is the most likely
      source of hostile diagnostics.** The split also matches the numeric rule
      the roadmap has already locked.

"Better" means **per-argument dominance, never a sum**. Summing ranks lets
`(Exact, Convert)` beat `(Alias, Alias)`, which no user can predict. A candidate
wins only if it is no worse on every argument and strictly better on at least
one. No C++-style partial ordering. Ambiguity is always an error.

Put candidate lists in `AnalysisError::hint`, not `message`, so substring
assertions stay stable.

## Two-pass `analyze()`

The overload set must be complete before any body is analyzed. Split the loop at
`analyze.rs:207-252` into declare / define. `map_type` does not consult the
scope, so pass A can register `fn default() Dog` before `Dog` exists.

**Risk:** forward references start working. That is strictly more permissive, so
no passing test can break — but a *negative* test asserting the error can. Grep
`analyze/test.rs` for `not found in scope` first.

## Duplicate detection

Three separate mechanisms:

1. Local declarations — `insert_overload`; a duplicate signature is a hard
   error.
2. Import copying — `merge_imported`; identical symbol and signature is a silent
   no-op. This is what keeps diamond imports and repeated `extern printf`
   working. Today it works by accident, because `insert` silently replaces.
3. `dedupe_declarations` — unchanged in shape, keyed on the now-unique symbol.

- [ ] Close duplicate **type** names too (`analyze/types.rs:128` currently
      replaces silently). The two-pass split makes that hazard worse, since all
      bodies would see the last declaration instead of the lexically preceding
      one.

## Tests

- [ ] Overload by parameter type, by arity, and by return type at each
      propagation site (annotated declaration, `return`, call argument, struct
      field initializer, assignment).
- [ ] Exact beats widening; an integer literal picks `int4` over `int8`;
      non-variadic beats variadic.
- [ ] `main` and `extern` stay unmangled; a local `helper` and an imported
      `mod::helper` both get emitted with the module's internal call intact.
- [ ] Determinism: analyzing the same source twice yields the same symbol
      sequence.
- [ ] Negatives: identical signatures; `extern` overloaded with a different
      signature; return-type-only overloads called in statement position;
      equally good candidates (pins dominance-not-sum); wrong arity; an
      overloaded name used as a value; an ambiguous call as a cast operand.
- [ ] `samples/overloads.fib` with a `#[test]` — the only end-to-end proof that
      the mangled symbols actually link.

## Known limitation, to document

`outer(inner())` where **both** are overloaded is not resolvable: arguments are
typed once, so the inner call has no expected type. Re-typing per candidate is
exponential *and* has side effects — instantiation writes into the cache, which
is drained into the output, so every losing candidate would emit dead code.

Return-type overloading in statement position (`make();` with
`fn make() Dog` and `fn make() Cat`) can never be resolved. Document it in
`docs/functions.md` rather than working around it.
