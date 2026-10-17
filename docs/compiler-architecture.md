# Compiler architecture sketches

This branch defines **interfaces and data models**, not a working compilation
pipeline. Tokenization and parsing work independently; project analysis,
`AnalyzedProject -> Ir` lowering, SSA construction/verification, and LLVM emission
are explicitly unimplemented. Their entry points return errors rather than fake
successful output. Build-file interpretation, object generation, and linking
remain separate unfinished work.

## Boundaries

```text
BuildPlan + parsed SourceUnits
            |
       Analyzer passes                 not implemented
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

Parsing preserves spelling and source spans. It does not assign resolved names,
infer even builtin expression types, or turn fields into lexical bindings.

## Analysis contracts

`Analyzer::new(&BuildPlan, Vec<SourceUnit>)` and `Analyzer::analyze` are sketches.
The intended pass order is:

1. Register only files named in the build plan and collect file declarations.
2. Resolve lexical names, file paths, visibility, and compile-time aliases.
3. Resolve semantic types and members on the same AST nodes.
4. Validate annotation completeness and initialization dependencies.
5. Return `AnalyzedProject { units, symbols, types }`.

`ResolveContext` combines the current file, shared `ProjectSymbols`, and a lexical
`SymbolTable`; namespaces are not scope stacks. `TypeContext` can resolve members
without inserting their spellings into lexical scopes. Basic scope storage exists,
but language-level lookup and redeclaration rules are not implemented.

Identities live separately from source strings:

- `DeclId` identifies a declaration within a file.
- `LocalId` and `LabelId` are intended to be project-unique allocations, in separate
  namespaces.
- `ResolvedName` identifies a file/declaration entity or local binding.
- Semantic `TypeId` indexes one project's `SemanticTypes`, not a backend layout.

See [name resolution](name-resolution.md) for the language rules that future
passes must enforce. Forward name visibility does not permit reading a later
uninitialized variable. Compile-time aliases do not allocate runtime storage.

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
entry points currently reject execution.

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

Useful checks at this stage:

```sh
cargo fmt --check
cargo check --no-default-features
cargo test --no-default-features
cargo check
cargo clippy --all-targets -- -D warnings
cargo build
cargo test
cargo test --all-features
```

The retained default `llvm` Cargo feature still brings in the existing LLVM 21
dependency. The adapter sketches themselves contain no LLVM implementation;
removing or replacing that dependency is not part of this change.
