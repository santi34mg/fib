# Compiler architecture sketches

Analysis/test status updated against the implementation on 2026-10-19. See the
[current implementation guide](agents/README.md) and [roadmap](roadmap/README.md).

The first executable compiler milestone now works: a single-file
`fn main() int32 { return 42; }` passes source analysis, owned SSA construction
and verification, LLVM verification, PIC object emission and host `cc` linking.
The supported subset now includes int32 helpers/calls, initialized int32/bool
locals, assignment, comparisons and conditionals. Broader runtime representations,
production Fib ABI, storage, arithmetic, source loops and build.fib are unfinished.

## Present baseline and orchestration

`driver::{parse_files, run_frontend, lower_to_ir, run_pipeline}` implement the
single-file scalar/conditional subset. `BuildPlan` explicitly enumerates files;
`BuildPlan::single_file` canonicalizes the input and chooses an executable beside
it. Check and IR goals need no LLVM; object/binary goals need LLVM 21, and binaries
also need host `cc`. Output is staged and published after success. Source-overwrite
paths are rejected. Unsupported programs fail explicitly.

`build --single-file PATH` supports scalar helpers and conditionals with an int32 main.
Build without that flag, `run`, `test`, build.fib interpretation and IR
interpretation remain unimplemented. Historical positional-file, include-root,
check/emit/link/release options and `compile_project`/`check_project` are absent.

## Planned build-file interpretation

The future `fib build` path will discover exactly `<cwd>/build.fib`, compile it in
an internal bootstrap plan through the normal frontend and lowering boundary,
verify its owned IR and interpret that IR to populate a host-owned plan builder.
The finalized `BuildPlan` will then enter the existing project pipeline. Relative
paths are anchored to the build-file directory; `--single-file` remains a separate
mode that does not execute a build file.

The interpreter boundary is a versioned Fib API using opaque host-owned state. It
must not expose Rust values or Rust ABI, return the current `BuilderObject` by
value, invoke LLVM, emit a host executable, use a JIT fallback or load native code
into the compiler. Source syntax for the distinguished `build` entry and API types
remains open until their semantic representations are defined. See the
[interpreted build-file plan](roadmap/08-interpreted-build-files.md) for staged
implementation, validation and acceptance criteria. This is intended architecture,
not current behavior.

## Working subset boundaries

```text
BuildPlan -> parse_files -> Analyzer -> AnalyzedProject
  -> lower_to_ir -> owned Ir -> verify_ir -> LLVM emitter
  -> LLVM verification -> object -> cc -> executable
```

The builder stages changes and verifies before publication. The scalar verifier
checks ownership, definitions, reachability/dominance and signatures across
blocks. The LLVM emitter supports multi-block SSA/PHIs, backedges, comparisons, direct
calls and branches/returns. Each signature selects its ABI; a shared description
controls declarations, incoming arguments, calls to the function and its returns.
Only C 32-bit integer/void ABI ships. Broader representations/storage and Fib ABI
remain unsupported.

There is no separate typed AST or direct AST-to-LLVM fallback. See the
[IR contract](../src/ir/README.md) and [LLVM backend](../src/backend/README.md)
for exact implemented limits. Broader design sections below describe intended
contracts and do not imply that all frontend constructs can be emitted.

## Parser and source identities

The tokenizer emits `Identifier(String)`. Parser identifier helpers return
strings; single and qualified expression paths both use `QualifiedIdentifier`.
Type-declaration dispatch consumes the `MetaType` builtin, not a separate `type`
keyword. Member-access spelling is `accessed_identifier`.

Current source uses bare bit-width types (`int32`, `uint64`; `int`/`uint`
mean 32-bit), `x := 1;`, `const x := 1;`, `x: int32;`, and
`fn f(x: int32) int32 { return x; }`. Loops use `while (condition) { ... };`.
Array/slice **types** still use suffixes (`int32[8]`, `int32[]`); value access
uses `a[i]`, `a[a..b]`, `.len` and postfix pointer operations (`p*`, `x&`).
Functions are ordinary identifiers; builtin functions and their `@` calls have
been removed. Primitive type values still parse in expressions.
Some display helpers/comments retain old spellings and are not grammar authority.
Legacy `@` types, byte-width `int4`, `var`, `for`, dot-index and import declarations
are not current working syntax. Generic parameter lists, prefix array types,
C-string literals, interfaces and constraints remain
[proposals](roadmap/README.md), not parsed feature guarantees.

`FileId`, `FileName` and paths live in `build_plan.rs`; `AnalyzedProject::source_paths` retains original plan paths for later diagnostics;
there is no central source-text store. Spans are point locations, not complete byte ranges.
`Parser::with_source` preserves in-memory diagnostic context.

Parsing preserves spelling and source spans. It does not assign resolved names,
infer even builtin expression types, or turn fields into lexical bindings.

## Analysis contracts

`Analyzer::new(BuildPlan, Vec<CompilationUnit>)` validates plan/unit IDs through
`ProjectSymbols::from_plan` and retains paths for diagnostics. Collection annotates
source-ordered file bindings and reserves private local compile-time type slots.
`resolve_names` resolves file aliases, traverses names on the original AST, and
validates name/binding completeness. The name stages and `resolve_types` run once in order;
failed passes cannot be retried. No additional project wrapper is introduced.
`Analyzer::analyze` returns the validated source project. The pass order is:

1. Register only files named in the build plan and collect file declarations.
2. Resolve lexical names, file paths, visibility, and compile-time aliases.
3. Resolve semantic types and members on the same AST nodes.
4. Validate type-pass annotations, definite initialization and structured
   control-flow completion.
5. Return `AnalyzedProject { units, symbols, types, source_paths }`.

`ResolveContext` combines the current file, shared `ProjectSymbols`, and a lexical
`SymbolTable`; namespaces are not scope stacks. `TypeContext` can resolve members
without inserting their spellings into lexical scopes. The name pass enforces
lexical order, scope lifetime, file fallback, alias visibility, and cycle detection.
Semantic typing, contextual expression inference and owner-based member lookup
now execute, along with explicit metatype specialization, bounded type-return
evaluation, initialization and structured flow; see [type-resolution rules](type-resolution.md).

Identities live separately from source strings:

- `DeclId` identifies a file binding or a private, lexically scoped compile-time type.
- `LocalId` and `LabelId` are intended to be project-unique allocations, in separate
  namespaces.
- `ResolvedName` identifies a file/declaration entity or local binding.
- Semantic `TypeId` indexes one project's `SemanticTypes`, not a backend layout.

See [name resolution](name-resolution.md) for the implemented rules and remaining
semantic boundary. Forward name visibility does not permit reading a later
uninitialized variable. Local value declarations instead resolve all initializer
expressions before introducing any new bindings, including all members of a
multi-binding. File aliases support forward chains; local aliases follow declaration
order and cannot reuse registered file names/roots. Type bindings require explicit
`comp`, including inline local types and aliases to those types. Aliases do not
allocate runtime storage.

### AST annotations

`frontend::analysis_annotation` defines pending slots embedded in the source AST:

| Syntax | Annotation |
| --- | --- |
| Expression | Semantic type, value/place category, optional denoted type |
| Qualified identifier | Resolved name identity |
| Type expression | Denoted semantic type |
| Function signature, parameter, type/variable declaration | Binding identity |
| Member selection and struct field | Resolved member identity/type |
| Enum pattern and payload binding | Member and local-binding annotations |

Every applicable slot starts as `None`; this is pending analysis, not an unknown
but valid semantic result. Function declaration identity lives once on its
signature. Multi-bindings reuse each variable declaration's slot. Named-label
syntax and the reserved generic-parameter string list need binding-bearing nodes
when their syntax is specified; they are not falsely marked as analyzed now.

The public records are currently raw data. Constructing an `AnalyzedProject`
struct does not establish that analysis or completion validation ran.

## Lowering and ownership

`driver::lower_to_ir(&AnalyzedProject) -> anyhow::Result<Ir>` is the only lowering
boundary. Its future context must map canonical frontend declaration/instance IDs,
semantic type IDs, and local bindings into IR-owned identities. Source names are
not runtime lookup keys. A mutable source binding can map to several SSA values.
Generic instances must be distinguished by declaration and type arguments.

The IR owns runtime types, function signatures, globals, constants, blocks, values,
and stack slots. Its IDs are not frontend IDs; dropping source ASTs must not make
IR data invalid. Functions retain explicit ownership of blocks and storage even
though records live in shared arenas.

SSA merges use block parameters and edge arguments. There is no implicit
fallthrough. Every block has a separate terminator, and each value has one typed
definition. Memory is explicit through addresses, loads, and stores; memory SSA
is not required for this initial design. No verifier or SSA algorithm has been
implemented by declaring these records.

## LLVM and future native code generation

The LLVM boundary consumes only `Ir` and an explicit target specification.
It plans verification, target layout, shared ABI classification, predeclarations,
block/PHI setup, emission, edge patching, and final LLVM verification. All phase
entry points currently reject execution. These contracts are compiled without
LLVM as well; `backend/lowering/next/ir_lower.rs` is an inactive legacy prototype
outside the module tree, still referencing removed APIs. It is not a supported
backend, compatibility layer or fallback.

The adapter must handle block arguments on parallel edges and loop backedges,
track actual predecessors after block splitting, and use target-derived aggregate
and pointer layouts. Declaration, definition, call, and return ABIs must agree.
It must not guess external signatures, silently insert terminators, or invent
LLVM semantic flags. These constraints also inform a future native backend;
LLVM types and conventions do not belong in the core IR.

Before execution is implemented, explicitly decide arithmetic/trap semantics,
layout and ABI policies, indirect calls/intrinsics, pointer validity, and how a
verified IR is exposed. The detailed subsystem plans record these open choices.

## Migration and validation

Obsolete analyzer, typed-AST lowering, flat instruction-stream builder, and direct
LLVM lowering bodies have been replaced by the new contracts rather than retained
behind compatibility aliases. Existing parser/tokenizer tests were migrated;
small tests cover pending annotations, model structure, and explicit unsupported
operations. Deleted integration tests were not restored. These tests demonstrate
contracts, **not end-to-end compilation**.

All analyzer tests and helpers are in
[`src/frontend/analyzer/test.rs`](../src/frontend/analyzer/test.rs). Its inline
`name_resolution` module has six independently runnable phases for files,
parameters/locals, references/functions, aliases, type/member names and traversal.
The name phases, type-resolution acceptance cases and all six former pending
completion cases pass, with additional completion regressions. See
[validation commands](agents/tests/README.md). These name-resolution
checks do not establish type correctness or executable compilation.

`tests/executable.rs` compiles, links and runs the scalar/conditional subset.
Samples and the experimental standard library are not automatically compiled
or run. Tests of explicit stub errors do not demonstrate successful analysis,
construction, verification, interpretation, layout or emission.

Useful checks at this stage (commands, not a claim they all pass):

```sh
cargo fmt --check
cargo check --offline --no-default-features
cargo test --offline --no-default-features
cargo run --offline --no-default-features -- --help
cargo check
cargo clippy --all-targets -- -D warnings
cargo build
cargo test
cargo test --all-features
```

The retained default `llvm` Cargo feature still brings in the existing LLVM 21
dependency used by the initial scalar LLVM emitter. Default-feature
checks require the compatible LLVM installation. Offline commands additionally
require cached dependencies. See the [validation guide](agents/tests/README.md)
for toolchain, formatting and environment limitations.
