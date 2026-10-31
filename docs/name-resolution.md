# Name resolution

The confirmed [package and visibility design](language-specification.md#names-packages-and-source-discovery)
uses package-rooted paths, source discovery, prelude type values and scoped
visibility. Prelude identifiers, scoped visibility and package-rooted lookup
are implemented. Package metadata is explicit in build plans; recursive source
discovery remains deferred. Installed std is registered before parsing, using
ordinary package environments and file namespaces.

The name pass runs on the original parsed AST through `Analyzer::new`,
`collect_declarations`, and `resolve_names`. It resolves bindings, file paths,
visibility, and compile-time aliases. Successful name resolution is not successful
compilation: supported representations lower across explicitly planned files. Complete analysis now
adds initialization and structured flow validation.
Semantic typing and members are described in [type resolution](type-resolution.md).

## Files and identities

The build plan supplies every visible file's `FileId`, canonical name, and path.
Registration rejects duplicate IDs/names and mismatched compilation units without
reading or discovering files. File names may contain `::`, such as `pkg::math`.

Collection assigns source-ordered `DeclId { file_id, index }` slots to file
bindings, including each member of a multi-binding. It annotates binding sites
without resolving their initializers. File declarations are forward visible;
that does not make reads of later uninitialized variables legal.

Inline local compile-time types receive additional declaration slots after all
of their file's top-level declarations. Those slots are absent from the file's
name map: `file::LocalType` cannot expose a lexically scoped type. Ordinary locals,
parameters, local alias binding sites, and switch payload bindings receive
project-unique `LocalId`s. Alias uses carry the canonical target's identity.

## Lookup and visibility

Unqualified lookup checks lexical bindings, current-file declarations, then
registered file names. Qualified lookup selects `root` or a declared dependency
in the requesting package's environment, then finds its package-relative file
path. File prefixes are normalized with NFC, full default Unicode case folding,
NFC again and hyphen-to-underscore replacement, while declarations
remain case-sensitive. Flat plans retain their longest-prefix compatibility path. A canonical file
path bypasses a same-spelled runtime local. Only files, including aliases to files,
can be intermediate namespaces; variables, functions, and types can be final targets.

File declarations default to file-private. `pub(dir)` permits access from the
containing directory subtree in the same package; `pub(root)` permits package-wide
access; `pub` permits external access through the current file registry. Scope
checks use explicit package/file regions before following alias targets. Public
aliases may re-export accessible private targets while keeping the original path
restricted. Dependency environments are package-relative: a consumer cannot name
another package's dependencies unless it declares them itself. Resolution uses
registered names, never include paths or recursive filesystem imports.

## Compile-time types and aliases

Type bindings require explicit `comp`:

```fib
comp Point: type = struct { x: int32, y: int32 };
comp Number: type = int32;
comp Alias := Point;
```

`Point: type = struct { ... };` still parses, but analysis rejects it. The name
pass recognizes explicit metatype annotations, inline type values, and references
to known type declarations. General expression typing is separate.

An inline type owns a declaration identity. An entity-reference initializer aliases
an existing file or declaration, without creating runtime storage. For example,
an alias to a mutable file variable denotes that variable's original identity.
`const` creates an independent value binding; it does not create an alias.
The name pass leaves computed `comp` initializers to typing; eligible bounded
`comp fn` type computations run there. Missing initializers are rejected. Compile-time
multi-bindings require one initializer per binding; runtime tuple arity is left
to typing.

File aliases support forward chains across files. A dependency walk records
unvisited, resolving, and resolved aliases, caches final targets, and reports a
cycle chain when it reaches an alias already being resolved.

Local aliases follow declaration order and cannot target parameters or runtime
locals. They can target previously introduced local compile-time types. A local
alias cannot reuse a registered file name or the first segment of a registered
file path, preventing ambiguity between aliases and canonical file paths.

## Lexical lifetime and traversal

A new local is introduced after its type names and initializer resolve. All
initializers in a multi-binding resolve before any member is introduced. Thus
`x := x + 1;` can refer to a file-level `x` on its right side; without an existing
binding it fails. Local aliases and inline local types have no forward visibility.

Redeclaring an active enclosing lexical binding is rejected. Sibling blocks and
separate functions can reuse spellings, and locals can share file-declaration
spellings. Function signature type names resolve before runtime parameters are inserted;
metatype parameters participate symbolically in signature lookup;
bodyless functions also receive parameter annotations. Blocks and switch arms
restore their scopes on both success and failure.

All current name-bearing expression, statement, and type children are traversed.
Field and variant spellings are members, never lexical bindings. The pass resolves
member receivers and constructor field values while leaving member, semantic type,
and expression annotations pending. Nested functions, reserved generic declarations,
and legacy unannotatable qualified-access nodes return explicit errors. Label
storage remains separate; label syntax and control-flow target legality are deferred.

## API and validation

Collection and resolution each run once, in that order. Failed passes cannot be
retried on the partially annotated analyzer. `Ast::resolve_name` is the shared file
traversal; callers of this lower-level helper must supply collected declarations
and normalized file aliases. `Analyzer::resolve_names` validates all applicable
name/binding slots before returning success. No additional project wrapper exists.

Source failures use `CompilerError` with the name category, available point span,
and owning path. Compilation units do not retain source text, so these diagnostics
do not invent source lines. Source spellings/spans and later-pass annotations are
preserved.

`Analyzer::analyze` runs type resolution after the name pass, then adds
initialization and structured flow checks. The six inline specification phases
in `src/frontend/analyzer/test.rs` exercise the working resolver; see
[validation](agents/tests/README.md).

## Compiler and installed source modules

One module can contain both parsed Fib declarations and compiler-owned declarations.
Registration allocates distinct file/declaration identities, rejects duplicate names
and records compiler operations by `DeclId`. Aliases preserve these identities.
`std::prelude` selects implicit aliases through public exports; the initial export
list is empty. `std` follows ordinary dependency-root protection, while primitive
prelude names retain their existing independent protection. See
[standard-library setup](standard-library.md).
