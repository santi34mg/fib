# LLVM backend

The [language specification](../../../../docs/language-specification.md) is implemented for prelude names, casts, shallow readonly views,
warned pointer field access and exhaustive switches. Concrete generic functions,
multiple source files and fixed-array values execute. Package-rooted lookup, scoped
visibility, installed std sources and Linux x86-64 syscalls execute. Recursive source
discovery remains deferred. Checked runtime indexing/slicing and constant-initialized
globals execute.

`emit_llvm(&Ir, &TargetSpec)` consumes only owned SSA IR. It runs `verify_ir`,
creates a target machine with LLVM 21/Inkwell, applies its triple and data layout,
predeclares functions, emits the supported instructions and returns textual IR
only after LLVM module verification. There is no frontend-to-LLVM fallback.

Private std pointer/word conversions emit LLVM `ptrtoint`/`inttoptr` after IR
verification requires typed pointers and uint64 words. They support the source
Linux x86-64 `sbrk` allocator; generic pointer construction preserves the selected
pointee type, including nested readonly qualifiers.

## Executable subset

The C entry ABI supports 32-bit integer parameters/results and void returns.
Internal Fib helpers support direct Bool, fixed-width integers, IEEE floats,
structs, enums and typed pointers as parameters/results, or void, using LLVM fastcc.
This limited direct convention is
not the full production Fib ABI or a general external C interoperability contract.
Definitions support multiple blocks, entry arguments, block parameters/PHIs,
Constant/direct Call/arithmetic/comparison/unary/numeric Cast instructions,
aggregate construction/extract/insert, stack/field addresses, loads/stores, and
Jump/Branch/Switch/Return/Trap/Unreachable. Comparisons use operand signedness and IEEE
ordered predicates, except NaN inequality is unordered. Bool supports equality.
Integer arithmetic wraps; runtime guards protect division, shifts and float-to-int
casts before LLVM can produce poison. Guards update PHI predecessor mappings.
See [scalar semantics](../../../../docs/scalar-semantics.md). LLVM identified unpacked
structs and target-derived GEPs implement field layout; slots allocate in entry.
Arbitrary memory accesses use alignment 1. Source null checks are explicit IR
branches/traps; unchecked intrinsic accesses have no guard. Field GEPs do not
promise inbounds. See [struct/storage semantics](../../../../docs/struct-storage-semantics.md).
Emission traverses reverse postorder, patches PHI inputs after all values exist,
and splits parallel branch arms into distinct LLVM predecessors. Backedges and
shuffled body block order are covered by executable hand-built IR tests.

## Per-function ABI interface

`emit_llvm(ir, target)` is the only public emission entry point. The ABI is
selected by each `Signature::calling_convention`; there is no compilation-wide
ABI switch or alternative emission entry point. `abi::classify_abis` dispatches
per signature and creates a `FunctionAbi` indexed by `FunctionId`. Classification
receives resolved target triple/layout and logical IR types, and works without
LLVM. Descriptions currently represent direct scalar parameter placement and a
direct/void result. Structs use direct LLVM aggregate arguments/results; physical
coercions and hidden results remain deferred.

A function's ABI governs every call to that function and every return from it.
`llvm_abi` translates descriptions into declarations, incoming argument decoding,
call argument packing/result decoding and return packing. Logical IR signatures,
SSA values and PHIs remain independent of physical argument positions. A
mixed-convention test uses a test-only reversed-argument policy for internal Fib
functions; the test permutation is separate from production scalar fastcc classification.
Source helpers select Fib convention in lowering and retain internal linkage;
their versioned canonical names encode persistent package/module/declaration
identities and concrete type arguments, never file or type table indices.
Legacy main/build entries select C. Extended main signatures use a generated
source wrapper with C `main(int32, **uint8, **uint8) int32`; source main uses the
ordinary internal Fib convention. The driver and source std handle process
arguments, environment maps and Result exit statuses before backend emission.
Neither convention silently falls back to another.

Enums use target-derived tag-plus-union layout, verified active-payload extraction and direct internal fastcc boundaries; see [enum semantics](../../../../docs/enum-semantics.md). Switch successor occurrences have separate LLVM predecessors for PHIs.

Broader C ABI widths, varargs, multiple results and unsupported instruction/
type families still return errors. General Fib ABI policy and source ABI syntax
remain future work.

`TargetSpec` explicitly requests a triple, CPU and features. Only `generic` CPU
with default features is supported; other CPU/features fail instead of being
ignored. LLVM resolves the target and data layout. `host_target()` explicitly
selects the host triple for the driver; invalid targets fail.

`LlvmArtifact` owns verified text plus triple/layout and target request. Its
private fields prevent constructing unchecked artifacts. `write_object(path)`
parses/verifies that text and uses LLVM's target machine to emit a PIC object.
The driver owns output paths, temporary files and linking with host `cc`.
Optimization and cross-linker orchestration are not implemented.

The API remains callable without the `llvm` feature: input verification works,
but LLVM emission, object output and host-target selection report explicit errors.

## Verification and remaining work

`ir_lower::verify_input` returns a borrowed proof after shared verification.
Working emission uses private state in `llvm_lower`.

Future implementation needs target-aware representations for all runtime types,
broader ABI classification and the remaining instructions. Parallel edges carrying
different arguments require distinct LLVM predecessors; backedges need deferred
SSA resolution. LLVM verification must remain mandatory. Never repair malformed
IR, infer missing declarations, fabricate layout sizes, invent runtime checks or
add overflow/alias/fast-math flags without defined and validated IR semantics.

`llvm_data` emits global constant initial data and typed address/slice relocations
before function bodies. Alternative physical enum payload layouts preserve
relocations and must match runtime sizes, alignments and offsets. Only constants
actually used as function operands are materialized into canonical SSA types;
global initializers alone do not allocate function stack storage. Array/slice
bounds checks arrive as explicit IR CFG, and no startup initializer is emitted.

`tests/executable.rs` compiles through the CLI, links and checks real exit codes.
Fixtures include `return_42.fib` and `conditionals.fib`; cases cover helper calls,
branches, multiple merged values, bool locals, nested scopes, early returns and
negative literals. Tests also check object-stage linking, unsupported source,
source protection and preservation of existing output on linker failure.

## Requirements for extending the backend

The following wider policies remain requirements for future implementation.
They do not expand the supported subset described above.

## 1. Verification is a gate, never a repair pass

Raw `Ir` records and successful arena lookups are not proof of validity. The
shared verifier checks the scalar subset described above and rejects unsupported
families. Extending support requires all applicable checks below:

- Arena references, stable IDs, ownership/body membership, canonical symbol
  uniqueness, declaration/definition consistency, type/constant validity and
  non-recursive-by-value layout prerequisites.
- Exactly one definition per `ValueId`, reciprocal definition metadata,
  instruction ordering and dominance, including edge operands on loop
  backedges. Values cannot cross function boundaries.
- Explicit terminators and same-function successors, entry having no incoming
  edge, edge argument arity/types, block parameter identities, unique switch
  cases and typed switch subjects.
- Instruction/result arity/types, pointer-typed memory addresses, global
  initializer types, exact call signatures and return result lists.

The backend then checks target support and the fully emitted LLVM module. It
must not silently add returns, invent unreachable terminators, fix argument
counts, truncate operands, synthesize declarations, replace missing values with
zero/undef/poison, or ignore unknown instructions. A diagnostic dump is not LLVM
IR and is not validation. CFG normalization for legal LLVM PHIs is an explicit,
semantics-preserving backend transformation, not a repair of malformed input.

## 2. Target data and type representations

Use validated target machinery/data layout, not Rust host `size_of`, a guessed
triple, or the old hard-coded `(8, 8)` pointer layout. Resolve pointer storage
size, ABI alignment, and GEP index width separately (including address spaces
when supported); function-pointer properties may require separate handling.
`IntegerWidth::Pointer`, slice lengths, pointer casts and pointer arithmetic
must use the target policy, not assume `i64`. Unsupported pointer models must
error, not fall back to the host.

Maintain mappings by IR `TypeId`, including distinct nominal struct/enum
identities. Separate SSA value representation, memory representation and ABI
representation. LLVM opaque pointers do not carry the IR pointee type: retain
that type to select load/store/projection types explicitly. Signedness comes
from operand IR types, even when a comparison's result is `Bool`.

Compute field offsets, padding, aggregate alignment, array stride, enum tag and
payload layout, and checked size arithmetic from target data. Boolean memory
representation, zero-sized types, slices, enum discriminant width and payload
alignment need explicit policies. A byte array sized for the largest payload
is not automatically aligned for it. Support pointer-mediated recursive types
through predeclared type identities; reject infinite by-value layouts. Do not
silently flatten aggregates or equate types by diagnostic names. Constants use
exact integer/float bits and exact byte arrays; never add a string terminator or
implicit allocation/load.

A future custom backend may share target-independent layout/ABI policy where
appropriate, but LLVM type handles and module ownership stay in its emitter.
Do not promise that every target has the same ABI or data-layout syntax.

## 3. ABI: one source of truth at every boundary

Classify each `Signature` and target/calling convention once, including
function-pointer types. The same plan determines:

- External declarations and internal/external definitions.
- Reconstruction of semantic entry parameters from physical arguments.
- Call argument packing, physical call type and semantic result reconstruction.
- Return packing and hidden-result writes.

Plan ignored/zero-sized arguments, direct scalars, split/coerced aggregates,
indirect parameters, `byval`, `sret`, required alignment, extension attributes,
hidden parameter order, and multiple semantic results. Void is an empty result
list, not a guessed scalar. Returning an LLVM struct is not a universal C
multi-result/aggregate ABI. Classification must distinguish Fib and C; neither
convention may silently stand in for the other. Attributes must agree on the
declaration, definition and call, including ABI-required attributes.

Variadic signatures are removed from Fib and its IR. External C variadic APIs
require separately supplied fixed-signature C shims, not guessed tail promotion
or a variadic LLVM declaration. Reject unsupported conventions/aggregate cases
explicitly. The present IR has direct calls by `FunctionId`, not an indirect-call
instruction; do not invent one or a runtime helper as a side effect of emission.

## 4. Predeclarations and identity maps

Predeclare all functions and globals by `FunctionId`/`GlobalId`, preserving their
canonical linker names, `Linkage`, declared signatures and declaration/body
status. This permits recursion, mutual recursion, forward calls and constants
containing global/function addresses. Then resolve constant graphs and attach
explicit initializers. `GlobalInitializer::Declaration` is not a zero-filled
definition, and external linkage does not imply absence of a body.

Missing IDs/signatures, symbol collisions and inconsistent redeclarations are
errors. Never call an LLVM "get or insert" facility with an inferred function
type, choose `i32 (...)` as a default, search source scopes, or create a helper
because a named function was absent. Runtime/intrinsic requirements must first
have a specified, validated contract. Stack slots are separate storage IDs;
SSA values are not variables secretly backed by allocas.

## 5–7. Block parameters, PHIs and the actual emitted CFG

1. Allocate a backend block-entry mapping for **every** block in each definition,
   independent of traversal order. There is no implicit fallthrough. Entry block
   parameters bind ABI-unpacked function arguments, never PHIs; entry cannot be
   the target of any IR edge.
2. Precreate PHIs for non-entry block parameters (or a representation-aware set
   of PHIs if a semantic value has multiple physical components), before body
   emission. Map each parameter `ValueId` to that representation. Ordinary
   instruction result IDs likewise have exactly one binding, including all
   results of a multi-result call; stores bind none.
3. Emit in a dependency-respecting traversal. Precreated PHIs support joins and
   loops; deferred fixups support backedge arguments whose values are not emitted
   yet. Block order in the arena does not establish dominance or define execution.
4. Record each **successor occurrence**. The emitter stores each IR edge and
   its actual emitted LLVM predecessor in an incoming-edge list, including
   jump, branch and switch successors with their positional arguments. A map
   keyed only by `(source, target)` loses parallel edges and is incorrect.
5. LLVM PHI operands name predecessor **blocks**, not branch-arm identities. For
   example, `branch cond, join(a), join(b)` with `a != b` cannot become two
   different PHI values from the same LLVM predecessor. Normalize these edges
   with distinct edge/trampoline blocks (`then.edge -> join`, `else.edge ->
   join`), or another explicitly proven equivalent transformation. The simple
   initial strategy can split every parallel successor occurrence. Switch cases
   and default need the same handling. If identical arguments allow unsplit
   repeated edges, LLVM's incoming-edge multiplicity and identical-value rules
   must still be satisfied; never deduplicate incoming entries blindly.
6. Track **actual emitted predecessors** after every backend block split. A
   logical source block may emit several LLVM blocks during legalization; the
   block that finally branches to the destination (possibly an edge trampoline)
   is the PHI predecessor, not necessarily its original block-entry mapping.
   Keep separate logical-entry/current-exit maps and update fixups when edges
   are rewritten. Preserve operand dominance along the final edges.
7. Once definitions and final edges are available, resolve every argument and
   populate the target PHIs positionally using final predecessor identities.
   Check arity/types and complete incoming-edge multiplicity against the emitted
   CFG; no stale predecessors, missing values, duplicate bindings or leftover
   fixups may survive. Handle self-loops, backedges, irreducible CFGs and
   unreachable blocks explicitly. A predecessor-free parameter block needs an
   explicit supported policy or error, not an arbitrary undef value.

Bounds checks, enum-active-variant checks and language traps must be specified
in the IR rather than invented here. Backend legalization splits preserve
already-defined semantics; they do not introduce a source-level release/debug
policy or repair absent language checks.

## Instructions and semantic flags

Preserve memory effect ordering, explicit loads/stores and value-vs-address
aggregate operations. Resolve numeric signedness and widths from operand types;
respect cast kinds and raw constant bits. Arithmetic overflow, division/shift
edge cases, floating comparison/NaN behavior, enum access preconditions and
pointer provenance still need explicit IR semantics (see `../ir/README.md`).
Unsupported or unspecified cases must fail rather than select convenient LLVM
semantics.

Do not guess `nsw`, `nuw`, `exact`, `inbounds`, fast-math, load/store alignment,
`nonnull`, `noalias`, `dereferenceable`, `noundef`, or memory-effect attributes.
Use flags/attributes only when guaranteed by the IR contract, validated ABI, or
a sound analysis. Known type alignment alone is not proof of alignment for an
arbitrary pointer operand. Do not add volatile/atomic behavior not represented
in the IR. Final LLVM verification is mandatory, but does not prove these
semantic promises are true.
