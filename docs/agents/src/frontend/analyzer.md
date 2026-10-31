# Analyzer

The [language specification](../../../language-specification.md) is implemented for prelude names, casts, shallow readonly views,
warned pointer field access and exhaustive switches. Concrete generic functions,
multiple source files and fixed-array values execute. Package-rooted lookup, scoped
visibility, installed std sources and Linux x86-64 syscalls execute. Recursive source
discovery remains deferred. Checked runtime indexing/slicing and constant-initialized
globals execute; value-returning `comp fn` execution remains scaffolding.

Current source is
[`analyzer.rs`](../../../../crates/fib-frontend/src/frontend/analyzer.rs) and `analyzer/`.

Struct function members have private declaration IDs excluded from lexical/file
lookup. `SemanticTypes::comp_members` maps nominal owner/name pairs to those IDs;
stored fields retain independent contiguous indices. Signatures are prepared
before member bodies, with explicit enclosing-type lookup for local type members.
Selections require a type value, produce ordinary function values, and never
infer or bind a receiver. Completion validation traverses member bodies and checks
their owner/identity mappings. Flow checking and lowering use the shared borrowed
`ast::members` traversal; no synthetic source functions or closure storage are used.

Collecting type analysis shares immutable declaration recipes, function AST
snapshots and source paths through `Arc` maps. Statement recovery checkpoints
still copy mutable inference state and semantic types, restoring both on failure;
sharing source snapshots does not reuse mutable annotations between analyses.

Rejected slice casts distinguish readonly removal from incompatible element
types and explain that converting elements requires copying into a new buffer.
Type compatibility diagnostics render builtin and container types with source
names and layer-local qualifiers instead of internal type-table indices.

Computed type annotations wrap ordinary type-returning calls. Name resolution,
the shared bounded evaluator, structural walks and completion validation all
traverse their expressions. Generic substitution recovers cached computations
for dependent nominal struct/enum results and recomputes them with substituted
arguments; this preserves identity between `Map(K, V)` annotations and generic
factory return values.

`Analyzer::new` validates plan/unit IDs using `ProjectSymbols::from_plan` and
retains source paths for diagnostics. `collect_declarations` assigns and annotates
file binding identities, visibility, and private local compile-time type slots.
It rejects duplicate file declarations and unexpected top-level statements without
resolving initializer names. Local type slots are excluded from file-name lookup.

`resolve_names` normalizes file aliases, traverses each original AST through
`Ast::resolve_name`, and validates applicable name/binding annotations. Collection
and resolution run once in that order; a failed pass cannot be retried on partial
state. There is no separate name-resolved project wrapper.
`Analyzer::resolve_types` runs after names, annotates types/expressions/members,
validates their completion and returns `SemanticTypes`. `Analyzer::units` exposes
those original units. The free type entry requires resolved names and fresh state;
`TypeContext::new` prepares lower-level node helpers. Failed type passes cannot be
retried. `Analyzer::analyze` runs typing, then validates initialization and
structured flow before returning `AnalyzedProject`, owning original units,
symbols, semantic types, cached concrete generic bodies/environments and original plan paths in
`source_paths` for diagnostics after analysis.

Enum payload expressions denote one type rather than an implicit payload struct.
`SemanticTypes::enum_payload` classifies omitted, void, empty-struct and recursively
unit-only struct payloads consistently for construction, switch bindings and
lowering. Declared types remain available for substitution and independent syntax
validation. Generic substitution and materialization refresh single-argument
constructor signatures; normalized unit variants have no constructor signature.
Optional unit bindings have type void, and validation independently checks every
switch binding's normalized type and mutable-place category.

## Identity and lookup

`DeclId` identifies file bindings and private local compile-time types. Project-wide
`LocalId`s identify parameters, runtime locals, local alias binding sites, and
switch payloads. Alias uses retain the canonical entity identity rather than
allocating new storage. `ProjectSymbols::resolve_path` follows registered files and
aliases with visibility checks; it never searches disk. Pending file aliases use
an explicit iterative dependency stack with cycle-chain diagnostics and cached
canonical targets; long chains do not recurse through the Rust stack.

Unqualified names prefer lexical bindings, then current-file declarations, then
registered file names. Qualified paths resolve `root` and dependency names using
the defining package's environment, then normalized package-relative file paths.
Flat plans retain longest-prefix compatibility.
Only files and aliases to files can be intermediate namespaces.

Local bindings and aliases follow declaration order. All multi-binding initializers
resolve before any new binding is introduced. Active enclosing lexical bindings
cannot be redeclared; sibling scopes can reuse names. Local aliases cannot reuse
registered file names or file roots, and cannot target runtime parameters/locals. Computed local `comp` values retain
lexical identities and can be aliased.

Type bindings require explicit `comp`, including inline local types and type
aliases. Local type aliases can target previously declared compile-time types.
Type/member checking is separate: name traversal does not insert fields/variants
into lexical scopes or fill semantic type, expression, or member slots. Scopes
restore on errors. Unsupported nested functions, reserved generic declarations,
and unsupported compile-time computations return errors.
Explicit metatype arguments specialize signatures and recheck dependent operations
in concrete bodies without replacing symbolic template annotations. Bounded
`comp fn` type-returning calls accept known type/bool arguments, declarations,
nested eligible calls and branches. Runtime function identity does not confer
compile-time eligibility. Deferred computations retain arguments/dependencies;
failed recursive instance groups roll back new cache entries. Type qualifiers
separate binding and element mutability; `never` coercion is directional.

The shared `constant_data` evaluator validates explicit file-level initializers
without executing runtime code or reading runtime bindings. Values retain semantic
types and addresses retain declaration/projection identities for lowering. Ordinary
calls are rejected even under short-circuit syntax. Value-returning `comp fn`
definitions are type/flow checked; their execution reports "not implemented".
Flow validation also checks constant bounds through `comp` values and requires
initialized arrays for dynamic element reads and slicing.

`SymbolTable`/`Scope` remain mechanical lexical utilities, including separate label
storage. Scope policy is enforced by the resolver. `ResolveContext` and `TypeContext`
separate pass inputs. Name errors use structured diagnostics with available spans
and owning file paths, without fabricating source lines.

## Tests and remaining boundary

All analyzer tests/helpers remain in
[`analyzer/test.rs`](../../../../crates/fib-frontend/src/frontend/analyzer/test.rs), with six inline
name-resolution phases. They pass for registration, locals, references, aliases,
type names, and traversal; [validation](../../tests/README.md) records commands.
Name completion does not imply semantic type validity, legal initialization,
control-flow legality, or executable code. Separate `type_resolution` tests cover
semantic typing/member lookup and validator corruption cases. All six former
pending cases now pass, with added initialization, flow, defer, specialization
and computed-type regressions.
See [type-resolution rules](../../../type-resolution.md). See [name-resolution behavior](../../../name-resolution.md) and
[semantic tables and annotations](semantic-types.md).

Source and compiler declarations share namespaces, including mixed modules. The
`ProjectSymbols::compiler_declarations` registry is keyed by canonical `DeclId` and
is shared by inference, independent validation and lowering. Installed std is
prepared before parsing; `Analyzer::new` attaches its intrinsic descriptors.
`with_module_intrinsics` augments a registered module through the same collection
path as source declarations. Synthetic module file IDs exclude every planned
source file, even when package-aware lookup uses separate file maps.
