# Semantic types

The [language specification](../../../language-specification.md) is implemented for prelude names, casts, shallow readonly views,
warned pointer field access and exhaustive switches. Concrete generic functions,
multiple source files and fixed-array values execute. Package-rooted lookup, scoped
visibility, installed std sources and Linux x86-64 syscalls execute. Recursive source
discovery remains deferred. Checked runtime indexing/slicing and constant-initialized
globals execute; value-returning `comp fn` execution remains scaffolding.

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
`Analyzer::analyze` after initialization and structured flow checks, retaining
original plan paths in `source_paths` for downstream diagnostics. Generic
instances are cached by declaration/type arguments with separately annotated
concrete bodies and owned binding facts. Templates retain symbolic type parameters
and deferred computations with arguments/dependencies. Readonly container layers
and explicit compile-time function eligibility participate in type identity.
`never` coerces only from a top-level found bottom type; composite compatibility
remains invariant.
Pointers, slices and fixed arrays implicitly gain an outer readonly qualifier;
nested qualifiers must still match exactly and readonly removal is rejected.
In expected slice-type contexts, the expression resolver rewrites fixed-array
values to ordinary full-slice expressions before checking compatibility. This
preserves backing places, initialization checks and readonly permissions, and
reuses existing slice lowering. Array literals receive the expected slice element
type. `check_assignable` remains a type comparison, not an array-to-slice coercion.
Implicit views do not copy elements or extend backing-storage lifetime.
Binding `const` protects nested struct fields and inline array elements through
their place categories, without changing value type identity. Array indexing
inherits immutable as well as readonly storage permissions; slicing such storage
produces a readonly view. Pointer and slice indirections retain their own type's
permissions, and inline value copies into mutable bindings remain writable.
The supported scalar/struct/enum/pointer/control-flow lowering translates frontend identities into
owned SSA IDs/types and never falls back to direct AST emission. General source
lowering remains unfinished.

See [type-resolution behavior](../../../type-resolution.md),
[analyzer](analyzer.md) and [validation](../../tests/README.md).
