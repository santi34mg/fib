# Compiler architecture

Reviewed against the current checkout on 2026-10-24. This document describes the
working pipeline and its extension boundaries. Detailed implementation references
live in the [agent guide](agents/README.md); future work belongs in the
[roadmap](roadmap/README.md).

## Pipeline and subsystem ownership

Fib analyzes the original source AST, lowers it into owned SSA IR, and sends that
IR to either LLVM or the build-file interpreter. There is one source-to-IR entry
point, `ir::lowering::lower`, shared by normal programs and bootstrap build files.
Analysis does not produce a separate typed AST or emit backend instructions.

```text
build --single-file PATH
  -> BuildPlan::single_file
  -> parse_files -> Analyzer::analyze -> AnalyzedProject
  -> validate main entry -> ir::lowering::lower -> verified Ir
  -> emit_llvm -> verified LLVM -> PIC object -> host cc -> executable

build --build-file PATH
  -> isolated bootstrap BuildPlan + synthetic build_api namespace
  -> parse_files -> Analyzer::analyze -> validate build entry
  -> ir::lowering::lower -> verified Ir -> interpret_ir -> PlanBuilder
  -> finalized project BuildPlan -> normal program pipeline
```

| Subsystem | Responsibility |
| --- | --- |
| `plan` | File identities, explicit source membership, single-file plans and interpreted build-plan finalization |
| `frontend` | Tokenization, parsing, lexical names, semantic types, specialization, initialization and structured flow |
| `driver` | Parsing planned files, selecting entry contracts and pipeline goals, object staging and host linking |
| `ir::lowering` | Validating the supported source subset and translating analyzed identities into scalar SSA |
| `ir` | Owned runtime records, transactional construction and shared verification |
| `backend` | Target selection, per-function ABI classification, LLVM emission and PIC object output |
| `ir_interpreter` | Narrow build-API execution against host-owned `PlanBuilder` state |
| `diagnostics` | Structured compiler errors, source positions and rendering |

`run_pipeline(plan, goal)` exposes `Check`, `Ir`, `Objects` and `Binary` goals.
`Check` runs complete analysis and executable-entry validation; it does not certify
that the program belongs to the lowering subset. `Ir` additionally lowers and
verifies SSA. `Objects` emits one object, and `Binary` links an executable.

## Build plans and executable entries

`src/plan.rs` owns `FileId`, `FileName`, `SourceFile`, `BuildPlan` and `PlanBuilder`.
A plan explicitly enumerates source files, their logical names, an entry file ID
and an output path. Parsing and analysis do not discover additional files from
qualified names or filesystem imports. `parse_files` validates plan names/IDs and
the presence of the entry file; `Analyzer::new` requires compilation-unit IDs to
match the plan exactly.

`BuildPlan::single_file` canonicalizes a source file, derives its logical name
from its UTF-8 stem, and chooses an executable beside it by removing the source
extension (`.exe` on Windows). It rejects inputs without a distinct output name.

The normal pipeline requires exactly one `main` definition with no parameters
and an `int32` result. Bootstrap compilation instead requires `build() int32`.
These entry checks belong to the driver. Source lowering currently accepts only
one source unit, even though plans and analysis support multiple planned files.
A successfully finalized multi-file plan therefore does not imply executable
cross-file support.

The CLI supports `build --single-file PATH` and `build --build-file PATH`,
requiring exactly one option. Bare `build`, `run` and `test` return explicit
errors. Automatic `<cwd>/build.fib` discovery remains future work.

## Source representation and diagnostics

The tokenizer/parser construct source AST nodes with spellings, point spans and
initially pending analysis slots. Ordinary and qualified identifier expressions
use `QualifiedIdentifier`; member selection remains separate from lexical name
lookup. Parsing alone establishes syntax, not semantic or runtime support.

Current source uses bare bit-width types, `int`/`uint` aliases for 32-bit integers,
`x := 1;`, `const x := 1;`, `x: int32;`, and
`fn f(x: int32) int32 { return x; }`. Loops use
`while (condition) { ... };`; `while () { ... };` is unconditional. Type bindings
require explicit `comp` during analysis. Array/slice types use suffix syntax,
value indexing/slicing uses brackets, and pointer operations are postfix.
See the [parsing reference](parsing/README.md) for the verified grammar snapshot.

`CompilationUnit` pairs an AST with its source `FileId`. `Parser::with_source`
provides filename/text context for parsing errors; `AnalyzedProject::source_paths`
retains plan paths for later diagnostics. There is no central retained source-text
store, and spans are not byte ranges. Lowering errors identify unsupported source
forms with Backend diagnostics, spans and retained paths.

## Analysis and semantic identities

`Analyzer::analyze` performs declaration collection, name resolution, semantic
typing, independent annotation validation, and definite-initialization/structured
flow checks. It returns
`AnalyzedProject { units, symbols, types, builtins, source_paths }`.
The original units remain annotated source ASTs. Staged collection, name and type
entry points run once in order; a failed pass cannot be retried on partial state.
`resolve_types` alone does not certify initialization or flow safety.

`ResolveContext` combines a current file, shared project namespaces, a lexical
symbol table and local-ID allocation. File namespaces are not copied into lexical
scopes. Resolution enforces declaration order for locals, file fallback,
visibility, alias identity and cycle checks. Local initializer expressions resolve
before their new bindings are introduced, including every initializer in a
multi-binding. Forward file-name visibility does not authorize uninitialized
reads. Compile-time aliases do not allocate runtime storage.

`TypeContext` and project-owned `SemanticTypes` resolve expressions and members
without inserting member spellings into lexical scopes. Structs/enums have nominal
identity; structural type families are canonicalized. Binding mutability and
pointee/element mutability are independent. Numeric literals retain arbitrary
integer magnitudes or decimal spellings until contextual validation and rounding;
typed numeric values require explicit conversions.

Analysis supports explicit metatype signature specialization, cached concrete
body rechecking, and bounded type computation through eligible `comp fn`
declarations. These facilities do not yet imply runtime generic lowering or a
general compile-time interpreter. Complete analysis also checks `never` bottom
coercion/divergence, partial initialization, branches, loops, enum-switch coverage
and lexical defer exits. See [type resolution](type-resolution.md) for exact rules
and limits.

### Identities and annotation slots

| Identity | Meaning |
| --- | --- |
| `FileId` / `FileName` | Plan-local source identity / logical namespace name |
| `DeclId` | Declaration slot in its owning file namespace, including private local compile-time types |
| `LocalId` | Binding identity allocated across one analyzed project |
| `ResolvedName` | Resolved file/declaration entity or local binding |
| Semantic `TypeId` | Index into one project's semantic types; not a runtime layout |
| IR IDs | Independent indices into one owned IR's arenas |

AST annotations record binding identities, resolved qualified names, denoted
types, expression types/value-or-place categories, and owner-based member facts.
Function identity lives on its signature; each multi-binding has its own binding
slot. Enum patterns and payload bindings retain member/local identities.
Applicable slots start pending and must be resolved consistently before handoff.
Label identities/annotations and bracket-style generic parameter syntax remain
reserved rather than implemented language features.

Public AST and semantic records can be constructed or modified directly;
`AnalyzedProject` is not an opaque proof type. `validate_analysis` independently
rechecks annotations, semantic facts, initialization and flow. Lowering invokes
that validation before constructing IR.

## Source lowering and SSA construction

`ir::lowering::lower` predeclares function identities and signatures, then checks
the entire supported source subset and gathers constants before building bodies.
Prevalidation includes dead branches, constant-false loop bodies and source after
control-flow exits. Unsupported syntax must fail even when emission would skip it.
Calls use resolved declaration-to-`FunctionId` mappings, not spelling-based lookup.

The executable subset supports signed/unsigned 8/16/32/64/128-bit integers,
float16/32/64/128 and bool; scalar helper parameters/results; void helpers; direct
calls; initialized locals; assignment; arithmetic/bitwise/shifts; explicit numeric
casts; comparisons; short-circuit logic; nested if/else; while; break/continue; and
returns. Source helpers use internal linkage and the limited scalar Fib convention;
the entry uses the C convention.

Mutable locals map to immutable SSA operands. Assignment replaces the current
mapping. Conditional joins merge differing continuing values through block
parameters; returning arms contribute no edge, and branch-local bindings do not
escape. Loops carry all bindings visible at entry through deterministically
ordered header/exit parameters. Continues and body fallthrough send current values
to the header; breaks send them to the innermost exit. Dynamic conditions execute
on every iteration, including their short-circuit CFG. Known-false loops emit no
body, and unconditional loops create an exit only for reachable breaks. Diverging
functions need no artificial return or unreachable block.

Uninitialized declarations, runtime globals, storage, aggregates, switches,
defer, indirect calls, multiple-result source functions and concrete generic
bodies remain unsupported by lowering. Analysis may accept these constructs;
that does not make them executable.

## Owned IR and verification

`Ir` owns types, functions/signatures, globals, constants, blocks, values and stack
slots. Records refer to IR IDs rather than AST nodes, semantic types or LLVM
objects. The IR can outlive source analysis. Functions explicitly own their
blocks and storage records even though arenas are shared across the IR.

Blocks have positional parameters, instructions and one separate terminator.
Edges carry positional arguments; arena order never implies fallthrough. Entry
parameters receive call arguments, and edges cannot target function entry.
Each SSA value has one typed definition and uses must obey dominance.

`FunctionBuilder` stages a cloned transaction, allocates owned records and rejects
insertion after termination or duplicate terminators. `finish` requires every
block to be terminated and verifies the staged IR before publishing. Failure or
dropping an unfinished builder leaves the original IR unchanged.

`verify_ir` checks supported types/constants, references, symbol uniqueness,
reciprocal ownership/definition records, signatures, instruction/result types,
edge arguments, reachability and dominance, including cycles. It rejects orphan
records and unreachable blocks. Supported operations include scalar constants,
direct calls, arithmetic/unary/comparisons/numeric casts and
jump/branch/return/trap/unreachable terminators. Interpreter-only Bytes are also
verified; LLVM does not lower them.

Pointer/aggregate/global/storage/switch records describe extension interfaces;
their presence is not verification support. Even allocated stack slots cannot
currently pass verification. Memory operations require future explicit
addresses/loads/stores rather than treating SSA local bindings as storage.
See the [IR contract](../src/ir/README.md).

## LLVM, ABI and artifacts

`backend::emit_llvm(&Ir, &TargetSpec)` is the public emission boundary. It verifies
IR, resolves an LLVM 21 target machine and data layout, classifies every function's
ABI, predeclares functions, emits scalar CFGs and verifies the final LLVM module.
`TargetSpec` carries a triple, CPU and features; only generic CPU/default features
are supported. The driver requests the host target.

ABI classification is per signature. One shared description governs declarations,
incoming parameters, calls and returns. The C subset supports 32-bit integer
parameters/results and void; internal Fib helpers use direct scalar LLVM fastcc.
Full production Fib ABI, broader external C interoperability, aggregates, varargs
and callbacks remain unfinished.

Emission allocates entry first, precreates non-entry PHIs, traverses reverse
postorder and patches incoming operands after definitions exist. Parallel branch
arms targeting one block get distinct trampoline predecessors. Runtime guard
splits update actual predecessor mappings, so PHIs remain correct across checks
and backedges.

Integer operations wrap; invalid division, shifts and float-to-integer casts trap.
Floating operations follow IEEE rules, with exact literal/constant evaluation and
explicit conversions. Guards precede LLVM operations that could otherwise produce
poison. These are shared scalar semantics, not optimization-dependent choices;
see [scalar semantics](scalar-semantics.md).

`LlvmArtifact` owns verified LLVM text and target metadata. Object writing
reparses/verifies it and emits PIC code. The driver stages object/binary output in
a temporary directory beside the destination, invokes host `cc` for binaries,
and renames output only after success. Source-overwriting paths are rejected;
linker failures preserve existing output and report stderr.

Without the default `llvm` feature, frontend/IR and ABI classification remain
available; host-target selection, LLVM emission and object output fail explicitly.
The separate backend context/type/general-instruction planning APIs remain
explicit-error sketches. `backend/lowering/next/` is inactive legacy code, not a
fallback. Broader layout, optimization and cross-linker orchestration remain
future work. See the [backend contract](../src/backend/README.md).

## Interpreted build files

`compile_build_file_to_ir` configures the ordinary analyzer with a synthetic
`build_api` module, validates `build() int32`, and uses the same source lowering.
Synthetic declarations have ordinary semantic identities but no source AST/path.
Their `DeclId` identities map to builtin functions and survive lowering through
an IR `FunctionId -> BuiltinFunction` table.

The interpreter verifies IR and supports a single returning entry block containing
direct `build_api::{add_source, set_entry, set_output}` calls with constant UTF-8
byte arguments. It dispatches by builtin identity into host-owned `PlanBuilder`
state. Multi-block/non-returning control flow is rejected before applying build
operations. General value evaluation, helper execution and CFG interpretation
remain unsupported, even when shared lowering can construct their IR.

Finalization anchors paths to the build-file directory, assigns deterministic
source IDs in declaration order, derives logical names from file stems, validates
the selected entry and output, and rejects duplicate paths/names, escaping paths,
missing sources, and source/build-file overwrites. Output parent directories must
already exist. The resulting plan enters the normal program pipeline, including
its current single-source lowering restriction.

Build-file execution uses neither LLVM nor native artifact loading. The versioned
opaque build API and broader interpreter capabilities are extended through this
same IR path; see the [build-file plan](roadmap/08-interpreted-build-files.md).

## Validation and extension discipline

Frontend tests cover syntax, names, types, specialization and analysis completion.
Driver tests exercise analyzed-source lowering, source diagnostics, scopes,
loop-carried values and build-file rejection. IR tests check construction and
malformed graphs; backend tests cover ABI classification and emitted CFGs.
`tests/executable.rs` compiles, links and runs scalar/control-flow programs,
including all numeric widths, guarded failures and bounded loop cases.

The parser sample sweep parses every top-level sample. `builtin_scalars.fib` and
`scalar_loops.fib` additionally have executable acceptance coverage. Other samples
and the experimental standard library are not automatically compiled or run.

The loop milestone passed 688 no-default-feature library tests, 695 LLVM-enabled
library tests and 16 executable acceptance tests, plus formatting, check and
all-target Clippy with warnings denied in both configurations. Rust 1.98.0 is
pinned by `rust-toolchain.toml`; the package uses Rust 2024 and Inkwell 0.8/LLVM 21.

```sh
cargo fmt --check
cargo check --offline --no-default-features
cargo test --offline --no-default-features
cargo clippy --offline --no-default-features --all-targets -- -D warnings
LLVM_SYS_211_PREFIX=/usr/lib/llvm/21 cargo check --offline
LLVM_SYS_211_PREFIX=/usr/lib/llvm/21 cargo test --offline
LLVM_SYS_211_PREFIX=/usr/lib/llvm/21 cargo clippy --offline --all-targets -- -D warnings
```

The LLVM prefix above is this environment's installation path; adjust it for
other systems. Offline commands require cached dependencies. For further checks
and CI gates see the [validation guide](agents/tests/README.md).

Extend support across analysis, source lowering, verification and the selected
consumer together. Raw representation records and positive parsing tests do not
establish executable behavior. Add meaningful positive/negative regressions,
preserve source identities and diagnostics, and keep unsupported operations as
explicit errors until their invariants and semantics are implemented.
