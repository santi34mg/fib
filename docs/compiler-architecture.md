# Compiler architecture sketches

Analysis/test status updated against the implementation on 2026-10-19. See the
[current implementation guide](agents/README.md) and [roadmap](roadmap/README.md).

This checkout defines **interfaces and data models**, not a working compilation
pipeline. Tokenization and parsing work independently. Analysis partially
validates named units, collects declarations and traverses name uses, but cannot
produce a completed analyzed project. `AnalyzedProject -> Ir` lowering, SSA
construction/verification, and LLVM emission remain explicit-error sketches. Build-file interpretation, object generation, and linking
remain separate unfinished work.

## Present baseline and orchestration

Working components are tokenization, parsing, source AST construction, lexical
scope storage, diagnostics, and raw IR data/access/formatting utilities. Scope
storage is not language-level name resolution; IR records are not verified SSA.

`driver::{run_pipeline, run_frontend, parse_files, lower_to_ir}` all return
explicit unimplemented errors. `BuildPlan` explicitly enumerates named files;
there is no working recursive import loader. Single-file loading, build.fib
interpretation, builder orchestration and IR interpretation are unfinished.
The CLI accepts `build [--single-file PATH]`, `run`, and `test`; help works,
but command execution is `todo!()`. Historical positional-file, include-root,
check/emit/link/release options and `compile_project`/`check_project` APIs are
not available.

## Intended boundaries (not an executable pipeline)

```text
BuildPlan + parsed SourceUnits
            |
       Analyzer passes                 incomplete
            |
     AnalyzedProject
            |
 driver::lower_to_ir                    not implemented
            |
       self-owned Ir
            |
       verification                    not implemented
            |
       LLVM adapter                    not implemented
       (future native backend)
```

There is no separate typed AST, old `IrProgram`, or direct AST-to-LLVM fallback.
The current `Ir` is the single backend-independent SSA representation. See the
[IR contract](../src/ir/README.md) and [LLVM adapter plan](../src/backend/README.md)
for the significant types, function signatures, invariants, and open decisions.

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
Builtin calls retain `@`, and builtin type values still parse in expressions.
Some display helpers/comments retain old spellings and are not grammar authority.
Legacy `@` types, byte-width `int4`, `var`, `for`, dot-index and import declarations
are not current working syntax. Generic parameter lists, prefix array types,
C-string literals, interfaces and constraints remain
[proposals](roadmap/README.md), not parsed feature guarantees.

`FileId`, `FileName` and paths live in `build_plan.rs`; there is no central
source-map store. Spans are point locations, not complete byte ranges.
`Parser::with_source` preserves in-memory diagnostic context.

Parsing preserves spelling and source spans. It does not assign resolved names,
infer even builtin expression types, or turn fields into lexical bindings.

## Analysis contracts

`Analyzer::new(BuildPlan, Vec<CompilationUnit>)` validates plan/unit file IDs and
creates the file-name registry. `collect_declarations` creates per-file declaration
slots and visibility flags, rejecting duplicates and invalid top-level statements;
it does not yet annotate binding sites. Name resolution partially traverses
initializers and expressions, but parameters, aliases, qualified target annotations,
access checks and many statement/expression branches remain unfinished. Pending
branches return errors or panic through `todo!()`. `Analyzer::analyze` still cannot
complete successfully. The intended pass order is:

1. Register only files named in the build plan and collect file declarations.
2. Resolve lexical names, file paths, visibility, and compile-time aliases.
3. Resolve semantic types and members on the same AST nodes.
4. Validate annotation completeness and initialization dependencies.
5. Return `AnalyzedProject { units, symbols, types }`.

`ResolveContext` combines the current file, shared `ProjectSymbols`, and a lexical
`SymbolTable`; namespaces are not scope stacks. `TypeContext` can resolve members
without inserting their spellings into lexical scopes. Basic scope storage exists,
but file fallback, alias/access rules and complete language-level scope handling
remain unfinished.

Identities live separately from source strings:

- `DeclId` identifies a declaration within a file.
- `LocalId` and `LabelId` are intended to be project-unique allocations, in separate
  namespaces.
- `ResolvedName` identifies a file/declaration entity or local binding.
- Semantic `TypeId` indexes one project's `SemanticTypes`, not a backend layout.

See [name resolution](name-resolution.md) for the language rules that future
passes must enforce. Forward name visibility does not permit reading a later
uninitialized variable. Local value declarations instead resolve all initializer
expressions before introducing any new bindings, including all members of a
multi-binding. This is the test specification, not the current local-insertion
implementation. Compile-time aliases retain forward-chain resolution and do not
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
The 217 specification tests include ordinary failures for unfinished behavior:
34 pass and 183 fail. The other 178 library tests pass when the specification is
excluded. See [validation commands](agents/tests/README.md); the full test command
currently exits unsuccessfully.

There is no checked-in integration/e2e harness. Samples and the experimental
standard library may contain stale syntax and are not automatically compiled or
run. Tests of explicit stub errors do not demonstrate successful analysis,
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
dependency. The adapter sketches themselves contain no LLVM implementation;
removing or replacing that dependency is not part of this change. Default-feature
checks require the compatible LLVM installation. Offline commands additionally
require cached dependencies. See the [validation guide](agents/tests/README.md)
for toolchain, formatting and environment limitations.
