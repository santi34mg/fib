# Analyzer — legacy analyze documentation URL

There is no `src/frontend/analyze/` implementation. Current source is
[`analyzer.rs`](../../../../../src/frontend/analyzer.rs) and `analyzer/`.

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
symbols, semantic types, cached generic signatures and original plan paths in
`source_paths` for diagnostics after analysis.

## Identity and lookup

`DeclId` identifies file bindings and private local compile-time types. Project-wide
`LocalId`s identify parameters, runtime locals, local alias binding sites, and
switch payloads. Alias uses retain the canonical entity identity rather than
allocating new storage. `ProjectSymbols::resolve_path` follows registered files and
aliases with visibility checks; it never searches disk. Pending file aliases use
an explicit iterative dependency stack with cycle-chain diagnostics and cached
canonical targets; long chains do not recurse through the Rust stack.

Unqualified names prefer lexical bindings, then current-file declarations, then
registered file names. Qualified paths prefer the longest canonical file prefix.
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
restore on errors. Unsupported nested functions, generic declarations, legacy
qualified-access nodes, and unsupported compile-time computations return errors.
Explicit metatype arguments specialize signatures; bounded type-returning calls
accept known type/bool arguments, declarations, nested calls and branches.

`SymbolTable`/`Scope` remain mechanical lexical utilities, including separate label
storage. Scope policy is enforced by the resolver. `ResolveContext` and `TypeContext`
separate pass inputs. Name errors use structured diagnostics with available spans
and owning file paths, without fabricating source lines.

## Tests and remaining boundary

All analyzer tests/helpers remain in
[`analyzer/test.rs`](../../../../../src/frontend/analyzer/test.rs), with six inline
name-resolution phases. They pass for registration, locals, references, aliases,
type names, and traversal; [validation](../../../tests/README.md) records commands.
Name completion does not imply semantic type validity, legal initialization,
control-flow legality, or executable code. Separate `type_resolution` tests cover
semantic typing/member lookup and validator corruption cases. All six former
pending cases now pass, with added initialization, flow, defer, specialization
and computed-type regressions.
See [type-resolution rules](../../../../type-resolution.md). See [name-resolution behavior](../../../../name-resolution.md) and
[semantic tables and annotations](../typed-ast.md).
