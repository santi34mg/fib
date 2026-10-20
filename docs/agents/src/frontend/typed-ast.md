# Semantic types — obsolete typed-AST path

There is no `typed_ast.rs`, `TypedProgram`, `TypedModule` or `Ty`. Syntax retains
its original spelling, spans, binding identities and in-place annotations.

`SemanticTypes` owns canonical builtin/composite types, nominal struct/enum
identities, value types/categories for declarations and locals, denoted types for
metatype bindings and enum constructor signatures. Pending reservations are
internal and must be finalized on a successful type pass. Alias normalization,
contextual expression inference and by-value size-cycle checks execute; runtime
layout/ABI and complete semantic analysis remain separate.

Names, bindings and owner-relative field/variant identities are distinct.
`check_assignable` is read-only compatibility of resolved types; the node
resolvers perform inference, literal checks, place checks and explicit casts.
`validate_type_annotations` rejects pending, invalid and inconsistent applicable
annotations and table entries. Namespace aliases have no fabricated value types.

`Analyzer::resolve_types` returns the semantic table after annotating the source
units; `Analyzer::units` exposes those units. `AnalyzedProject` is returned by
`Analyzer::analyze` after initialization and structured flow checks. Generic
signature instances are cached by declaration/type arguments; symbolic type
parameters and type-result computations remain represented in template bodies.
Planned lowering translates frontend identities into owned SSA IDs/types and
never falls back to direct AST emission.

See [type-resolution behavior](../../../type-resolution.md),
[analyzer](analyze/README.md) and [validation](../../tests/README.md).
