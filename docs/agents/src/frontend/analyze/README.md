# Analyzer — legacy analyze documentation URL

There is no `src/frontend/analyze/` implementation. Current source is
[`analyzer.rs`](../../../../../src/frontend/analyzer.rs) and `analyzer/`.
Every analysis pass and helper is an explicit unimplemented-error sketch.

Planned order: validate source units against the named build-plan files, collect
file declarations for forward references, resolve names/aliases, resolve
types/members, validate initialization and completion, return `AnalyzedProject`.
That project owns original source units, `ProjectSymbols`, and `SemanticTypes`.
No type inference, semantic coercion, generic instantiation, visibility checking,
alias-cycle detection or all-path return analysis runs today.

## Identity and scope contracts

`identifier.rs` separates `DeclId { file, index }`, `LocalId`, `LabelId`, `Entity`
and `ResolvedName` from spelling. `namespace.rs` describes per-file declarations,
visibility and aliases. `ProjectSymbols::from_plan` and `resolve_path` error;
resolution is intended to select only plan files, never search disk.

`SymbolTable`/`Scope` are working lexical utilities: permanent module root,
inner-to-outer lookup, scope kinds, duplicate rejection without replacing identity,
shadowing across frames, scoped aliases and separate label lookup. They do not
implement declaration order, no-capture function boundaries, file visibility or
label legality. `ResolveContext` and `TypeContext` separate name/type pass inputs;
member lookup must not create lexical bindings.

Per-concern sketches cover AST, expressions, statements, functions/signatures,
types/declarations, variables, generics and `swtich_arm.rs` (actual spelling).
[Semantic tables and annotations](../typed-ast.md) replace the removed typed AST.

`analyzer/test.rs` tests utility behavior, pending defaults, identity separation,
and explicit failure/no mutation from sketches. It is not the old semantic
behavior suite. Add real positive/negative language tests as passes become real;
never claim soundness coverage from an unimplemented-error assertion.
