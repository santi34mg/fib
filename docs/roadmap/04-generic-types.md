# 04 — Generic Data Types

**Depends on:** [03](03-prefix-array-syntax.md). **Blocks:**
[05](05-remove-string.md), [07](07-interfaces.md).
Implements Phase 7 "Add Generic Data Types".

## Goal

```fib
type Vector[T] struct {
    ptr: *T,
    len: usize,
    cap: usize,
}

v: Vector[int4] = Vector[int4] { ptr: p, len: 0, cap: 8 };
```

Use sites are always explicit. `struct` stays mandatory.

## What 03 gives us for free

With prefix arrays, a **postfix** `[` after a name can only be generic
instantiation. The LL(2) disambiguation between `Vector[int4]` and `int4[8]`
disappears entirely, and `Vector[int4]`, `[8]Vector[int4]`, `[]Vector[int4]` all
read without lookahead.

## Representation: no new `Ty` variant

`map_type` on a `GenericInstance` returns
`Ty::Identifier("option__Option__int4")`. The mangled name is registered as
`TypedSymbol::Type(Ty::Struct{..})` in the **global** frame and emitted as a
`TypedDecl::Type`.

This is the load-bearing decision of this document:

- `Ty` derives `PartialEq` and is compared structurally throughout `coerce_to`.
  A new variant means auditing every `==`. A mangled `Ty::Identifier` reuses
  `resolve_type_alias`, `resolve_struct_fields`, `typed_type_size_align`, and
  `map_type_to_llvm` **without a line of change**.
- Nominal identity falls out correct: `Vector[int4] != Vector[int8]`.
- Recursion terminates: `next: *Node[int4]` becomes
  `Ty::Pointer(Ty::Identifier("Node__int4"))`, which is finite. Structural
  inline expansion could not represent it at all.
- Nothing reaches LLVM: `TypedDecl::Type` emits nothing in either backend
  (`ir/mod.rs:308-310`, `backend/lowering/llvm_lower.rs:141`), and struct types
  are anonymous — `StructConstruct` lowering ignores `type_name` and uses
  `inferred_type`.

The backend resolves `Ty::Identifier` through `TypedProgram::symbol_table`
(`backend/lowering/types.rs:78-88`), while `SymbolTable::insert` writes to the
**innermost** frame — so a new `insert_global` is required.

## Threading

`map_type` (`analyze/types.rs:44`) takes no scope and no cache, and has 17 call
sites. `generic_cache` is hand-threaded through ~40 signatures (71 mentions).
[06](06-function-overloading.md) and [07](07-interfaces.md) each want to add
another channel.

Introduce `AnalyzeCtx { functions, types, in_progress, module_path, … }` here,
as a standalone mechanical commit with no behavior change, and change
`map_type(te)` to `map_type(te, scope, ctx)`. Later documents then add fields
with zero signature churn.

## Work

### Parser and AST

- [ ] Type-parameter list after the declared name in
      `parser/type_declaration.rs:13-25`. No ambiguity: a type expression can
      never begin with `[`, so a `[` right after the name is always a parameter
      list.
- [ ] `TypeExpressionKind::GenericInstance { module, name, args }` — flat, not a
      boxed head, so `(*T)[int4]` is unrepresentable. `Display` prints
      `Vector[int4]` / `option::Option[int4]`.
- [ ] `TypeDeclaration` gains `type_params`, `implements` (empty until
      [07](07-interfaces.md)), and `span` — the last closes the
      `declaration_line` gap documented at `analyze.rs:107-119`.
- [ ] Reject `type Vec[T: Default]` with "generic constraints are not supported
      yet" rather than silently parsing dead syntax.
- [ ] `StructConstruct` and `EnumVariantConstruct` gain `type_args`. Note struct
      construction is decided in `parser/primary.rs:62` on the identifier, not
      in `parse_postfix`, so `Vector[int4] { … }` is handled there.

### Analyze

- [ ] `TypedSymbol::GenericType(GenericTypeTemplate { name, defining_module,
      type_params, body, implements, span })`.
- [ ] `SymbolTable::insert_global`.
- [ ] Instantiation from `map_type`: resolve template, check arity, map
      arguments, mangle, consult cache, substitute, map the body, register
      globally, emit a `TypedDecl::Type`.
- [ ] Include the defining module in the mangled name (roadmap L377). Without
      it, `a::Option[int4]` and `option::Option[int4]` collide on `decl_key` and
      `dedupe_declarations`' last-wins silently miscompiles.
- [ ] Reject a mangled name that collides with a user-declared type — `__` is a
      legal identifier substring.
- [ ] Emit instantiated types sorted by mangled name, mirroring
      `analyze.rs:258-264`.
- [ ] `FieldAccess` needs a `TypeValue` arm beside the `TypeName.Variant`
      special case (`analyze/expressions.rs:881-904`) so `Option[int4].None`
      works.
- [ ] `substitute_in_expr` must substitute the new `type_args`, or
      `Vector[T] { … }` inside a generic body leaks `T` into the instantiation.
- [ ] `substitute_type` must recurse into `TypeExpressionKind::Enum`, which it
      does not today (it falls into the `_` arm at `generics.rs:124`).

### Two termination guards, for two different failures

- [ ] **Infinite size** — `type Node[T] struct { next: Node[T] }`. Detect with
      an `in_progress` stack plus a `Value`/`Indirect` position parameter:
      struct fields, enum payloads, tuple elements, and array elements propagate
      `Value`; the inside of `Pointer`/`Slice`/`Function` propagates `Indirect`.
      Re-entering the same mangled name in `Value` is the error; in `Indirect`
      it is legal recursion. Without this, `typed_type_size_align` overflows the
      stack.
- [ ] **Unbounded depth** — `type Wrap[T] struct { inner: *Wrap[Wrap[T]] }`.
      Every level has a *different* mangled name, so the occurs check never
      fires and the pointer defeats the size check. Only a depth cap works.
- [ ] Apply the same cap to `instantiate_generic` for **functions**, which has
      none today: polymorphic recursion diverges the compiler. This is a
      liveness bug and ships with a negative test.

### Cross-module, with a bounded contract

- [ ] Instantiate in the **consumer's** scope from the template in
      `module.exports`, registering the result flat under the fully qualified
      mangled name.
- [ ] After mapping, walk the result and reject any `Ty::Identifier` that does
      not resolve in the consumer, with a diagnostic naming the offending type.
      A template body may reference only type parameters, builtins, and other
      generic instances. This covers `Option[T]` and `Result[T, E]`, which is
      what Phase 7 asks for, and gives an honest error for everything else
      instead of a backend crash.

### Pre-existing defects to close here

- [ ] `resolve_struct_fields` (`analyze/types.rs:25-42`) does not handle
      `Ty::QualifiedIdentifier`, which breaks cross-module struct field access
      today. Mirror `backend/lowering/types.rs:86-96`.
- [ ] `resolve_type_alias` (`analyze/types.rs:12-23`) has no cycle guard, so
      `type A B` + `type B A` hangs the compiler.

## Tests

- [ ] `Vector[int4]` and `Vector[int8]` produce two distinct sorted
      `TypedDecl::Type` entries; a repeat hits the cache.
- [ ] Nested `Vector[Vector[int4]]`.
- [ ] Generic enum: `Option[int4].Some{…}` plus a `switch` over it.
- [ ] `T` substituted through a struct field **and** an enum variant payload.
- [ ] Negatives: infinite size; depth cap; wrong arity; bare `Vector` without
      arguments; alias cycle; mangled-name collision; a recursive generic
      *function* exceeding the cap; a template body naming a type not visible in
      the consumer.
- [ ] Driver test: a diamond import yields exactly one
      `type:option__Option__int4`.
- [ ] Samples with `#[test]` entries: `generic_struct.fib`,
      `generic_enum.fib`, `generic_recursive.fib`.

## Deferred

Constraints (see [07](07-interfaces.md)), a type-collection pre-pass,
higher-kinded or defaulted type parameters, and inference at use sites.
