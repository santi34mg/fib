# 00 — Canonical cross-file linkage names

**Status: implemented.** Related: [name resolution](../name-resolution.md) and
[overloading](06-function-overloading.md).

## Package and module identity

`PackageId`, `FileId`, declaration slots and semantic type IDs are build-local
handles. `PackageSpec::identity` is the persistent, case-sensitive acquisition
identity supplied by the host: for example `registry:example.org/math@1.2.0`
or a repository URL plus immutable revision. It must include the version or
revision that distinguishes installations. Empty identities, control characters,
and duplicate identities in a plan are errors. Dependency diamonds refer to the
same package registration; hosts must not register a second copy of that identity.
Different revisions may coexist under distinct dependency names. Acquisition and
download orchestration remain part of the build-system roadmap, not linkage.

Identity never includes the checkout path or the consumer's dependency alias.
The single application compatibility adapter uses `local:application`; independently
linkable packages must supply their own identity. Installed std uses
`fib:std@<compiler-version>:decl<declaration-revision>`.

Modules use package-relative paths with the source extension removed. Legacy flat
plans instead use normalized `SourceFile::name` values. Namespace segments
replace hyphens with underscores, apply NFC, full default (non-Turkic)
Unicode case folding, then NFC again. The tables are pinned by exact dependency
versions (`caseless` 0.2.2, Unicode 16 folding; `unicode-normalization` 0.1.26,
Unicode 18 normalization). Changing those tables requires an explicit compatibility
review. Paths must be UTF-8. Invalid segments, normalized collisions and
file/directory conflicts are errors. Declarations and locals remain case-sensitive.

## Symbol encoding v1

`lowering/linkage.rs` owns the encoding. A string component is its UTF-8 byte
length in decimal, `_`, then the original bytes. Length-delimited components
avoid punctuation ambiguity without lossy escaping or hashes.

- Ordinary functions: `fib.v1.function.` followed by package, module and
  declaration-name components.
- Globals: the same identity under `fib.v1.global.`.
- Generic instances: `fib.v1.instance.`, the template declaration identity,
  `A<count>_`, and length-delimited canonical type keys.
- Anonymous constant backing storage: `fib.v1.literal.`, its module identity,
  source location and canonical storage type. These are private implementation
  symbols; moving an anonymous source site may change its name.

Type keys resolve aliases and encode builtin kinds, pointer/readonly layers,
array lengths, slices, tuples and complete function types. Named nominal types
use their defining declarations. Computed nominal types retain the computation
that created them and its concrete arguments, including unused parameters;
later aliases or forwarding computations cannot rename them. Anonymous nominal
types retain source sites, enclosing specialization arguments and member types.
Recursive anonymous graphs use traversal-relative backreferences, never type IDs.

Definitions and calls share declaration/instance-to-`FunctionId` maps. Globals
and address relocations share `GlobalId` maps. Both maps are populated before
bodies and initializers; neither aliases nor diamond edges add definitions.

## Entry, foreign symbols and compatibility

The designated executable entry is `main`; the build evaluator keeps `build`.
Other functions with those source names receive ordinary canonical names.
`callconv "C"` declarations and definitions preserve the declared symbol exactly.
All duplicate emitted symbols, including compatible foreign redeclarations,
incompatible signatures and foreign/entry collisions, are errors before emission.
Declare a foreign symbol once and share it through aliases or re-exports.

Fib helper/global symbols remain internal: source `pub` controls language
visibility and does not export a native ABI. The v1 spelling is deterministic
under checkout relocation, local ID renumbering, equivalent namespace spelling,
file ordering and dependency alias changes. This is not a promise of identical
object bytes or a stable production Fib ABI. Incompatible future encodings must
use a new version prefix; C symbols follow their explicit ABI contract.

## Acceptance coverage

- [x] Declaration collection, aliases, package-relative lookup and visibility.
- [x] Persistent acquisition identities and normalized Unicode module identities.
- [x] Canonical declaration and generic-instance encoding without build-local IDs.
- [x] Function/global predeclarations and shared call/address identity maps.
- [x] Relocation, reordered/renumbered plans, aliases, diamonds, nominal types,
      generic local types, foreign redeclarations and entry collisions.
- [x] Executable diamond calls, same-spelled helpers and native C symbol linkage.

`tests/linkage.rs` checks the identity invariants and executable boundaries;
`tests/executable.rs` additionally covers cross-file globals and relocations.
