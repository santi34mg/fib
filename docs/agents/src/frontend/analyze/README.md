# Analyzer — legacy analyze documentation URL

There is no `src/frontend/analyze/` implementation. Current source is
[`analyzer.rs`](../../../../../src/frontend/analyzer.rs) and `analyzer/`.
`Analyzer::new` validates plan/unit file IDs and builds a file-name registry;
`collect_declarations` collects file bindings, visibility flags and declaration
slots, rejecting duplicates and unexpected top-level statements. Collection does
not yet annotate binding sites. These are partial implementations, not a completed
analyzer. `ProjectSymbols` registration/path helpers remain explicit-error stubs.

`resolve_names` walks file initializers and function bodies, but function signature
resolution still errors. Expression traversal handles literals, unary/binary nodes,
calls and lexical lookup; unqualified file fallback is missing and qualified lookup
discards the found target without visibility checks or annotation. Local insertion
currently precedes its initializer and omits binding annotations. Other branches
contain `todo!()` panics or explicit errors. The specification tests expose these
unfinished behaviors rather than treating them as language policy.

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

All analyzer tests and fixtures live in
[`analyzer/test.rs`](../../../../../src/frontend/analyzer/test.rs). Its inline
`name_resolution` module groups six independently runnable specification phases:
files/declarations, parameters/locals, qualified references/functions, aliases,
type/member names, and control-flow/traversal. Utility/default and unfinished
AST/typing contracts remain at the test module's top level. There are no separate
phase files. The specification deliberately fails on unfinished semantics; see
[test commands and validation](../../../tests/README.md).

Local value initializers resolve before introducing new bindings; multi-binding
initializers all resolve before introducing any member of the group. File-level
forward visibility and alias chains remain separate contracts. Type checking and
initialization validation are not executed by these tests.
