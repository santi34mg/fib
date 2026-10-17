# 00 — Canonical cross-file linkage names

**Status:** future identity/linkage design. **Related:**
[06 — Overloading](06-function-overloading.md).

## Baseline and historical motivation

The old compiler could derive a call symbol from an import alias while emitting
the definition under another name. That is historical motivation, not a
reproducible link failure in the current checkout: analysis, lowering and linking
are unfinished. Removed `TypedFunction`, `TypedModule`, `decl_key`,
`dedupe_declarations`, `analyze_in_module` and `--emit=typed` are not APIs to patch.

Current contracts use explicit `BuildPlan` file membership, `FileId`, `DeclId`,
`ResolvedName`, and project symbol tables. Qualified paths parse, but lookup does
not execute. Follow [planned name resolution](../name-resolution.md), not legacy
recursive imports. The [architecture](../compiler-architecture.md) maps identities
into owned IR functions/globals before the backend consumes them.

## Proposed invariants

- One canonical identity per defining declaration, independent of source aliases
  and how many paths reach it. Generic instances additionally include canonical
  type arguments, never just diagnostic names.
- Derive linkage names once from canonical identities; definitions and call sites
  consume the same mapping. Keep source names for diagnostics.
- Specify stable file/package identity before promising reproducible symbols
  across checkout locations. `FileId` alone is not a persistent linker spelling.
- Preserve declared foreign symbols and the designated entry symbol according
  to explicit ABI/entry policy. Detect collisions and incompatible redeclarations.
- Never deduplicate unrelated declarations by bare spelling or synthesize a
  missing callee signature in the backend.

## Work and acceptance tests

- [ ] Implement declaration collection and alias/file resolution first.
- [ ] Define canonical symbol encoding, instance keys and collision handling.
- [ ] Carry identities through AST annotations and `lower_to_ir` maps into
      `FunctionId`/`GlobalId`; predeclare before emitting bodies.
- [ ] Test two local names in different files, multiple aliases to one entity,
      repeated/diamond references, foreign symbols and entry policy.
- [ ] Once an execution harness exists, prove cross-file calls link and run,
      with one definition per canonical entity and deterministic output.

Renaming future symbols can affect foreign consumers; specify that compatibility
policy before emission ships. Old sample/import counts are not regression proof.
