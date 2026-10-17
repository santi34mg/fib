# Backend architecture sketch

The only compiler boundary is **`AnalyzedProject -> Ir -> backend`**. The
frontend/driver owns `AnalyzedProject -> Ir`; this directory consumes the
self-owned SSA model exported by `crate::ir`. There is no typed-AST backend,
flat `IrProgram` adapter, name-resolution fallback, or direct frontend-to-LLVM
escape hatch. A future custom machine-code backend will consume the same IR,
not LLVM text or an LLVM-shaped replacement for the language IR.

**No emission is implemented.** All unfinished operations return
`BackendError::NotImplemented { operation }`, including verification, target
resolution, and emission of an empty or declaration-only module. No `todo!`,
panic, dummy success, empty artifact, or partial output substitutes for a phase.
The opaque plan records describe dependencies, not working LLVM state machines.
Their fields and public APIs do not yet implement maps or enforce phase order.
No code invokes the pipeline yet, including `emit_llvm` itself.

## Public contract

```rust,ignore
pub fn emit_llvm(ir: &Ir, target: &TargetSpec)
    -> Result<LlvmArtifact, BackendError>;

pub struct TargetSpec {
    pub triple: String,
    pub cpu: String,
    pub features: Vec<String>,
}
```

`backend::{emit_llvm, TargetSpec, LlvmArtifact, BackendError}` is the intended
caller-facing API. `lowering` exposes architectural phase boundaries for later
implementation. No signature imports frontend types or LLVM handles. These
sketches therefore compile with or without `llvm`; actual LLVM machinery may
be feature-gated later without hiding unfinished operations behind success.

`TargetSpec` is an explicit, **unvalidated request**, with no implicit host
selection. `LlvmArtifact` owns textual LLVM IR and its resolved target triple
and data-layout string; it is not a path, LLVM context/module borrow, object
file, executable, or custom-codegen artifact. Its fields are private, with
`ir_text()`, `target_triple()`, `data_layout()` and `into_ir_text()` accessors.
Only a future emitter may construct it, after verification; the module text
and metadata must agree. The driver will separately choose where to write it,
how to invoke LLVM/object generation, and how to link. This layer does no I/O,
linking, target autodetection, or optimization today.

## Phase order and ownership

| Phase | Sketch API | Required future work |
| --- | --- | --- |
| 1. Verify input | `ir_lower::verify_input(&Ir)` | Invoke the shared `crate::ir::verify_ir`; return a borrowed `VerifiedIr` only on success. |
| 2. Resolve target/layout/types | `types::resolve_target`, `types::plan_types` | Validate the request; derive target data; build storage/value representations keyed by `TypeId`. |
| 3. Classify ABI | `abi::classify_abis` | Build one signature/legalization plan shared by every boundary crossing. |
| 4. Predeclare | `context::predeclare` | Create all function/global identities before constants, initializers or bodies. |
| 5. Blocks and PHIs | `ModulePlan::setup_blocks_and_phis(FunctionId)` | Create every block entry and non-entry block-parameter PHI; bind entry arguments and stack slots. |
| 6. Emit body | `FunctionPlan::emit_instruction`, `emit_terminator` | Bind SSA results and record unresolved incoming edges, using the final emitted CFG. |
| 7. Patch edges | `FunctionPlan::patch_edges` | Resolve forward/backedge operands, normalize parallel edges, populate PHIs and check completeness. |
| 8. Verify and serialize | `ModulePlan::finish` | Require every definition complete, verify LLVM, then produce owned text. |

`TypePlan` retains the verified input and resolved target; `AbiPlan` retains the
type plan; `ModulePlan` retains the ABI plan; a `FunctionPlan` exclusively
borrows its module. IDs remain scoped to this immutable IR. Future internal
state must reject wrong-owner IDs, invalid instruction indices, repeated
emission, premature finalization, dropped unfinished functions, missing
results/terminators and unresolved fixups. The sketch deliberately cannot
construct these opaque plans successfully yet. Implementing one phase must not
make a later unfinished phase appear successful.

Future failures need structured verification, invalid-target, unsupported-ABI,
unsupported-instruction, missing-ID, incomplete-emission and LLVM-verification
diagnostics with function/block/instruction context. Preserve upstream verifier
errors, including its current explicit unimplemented error. Do not flatten a
failure into "try the typed AST instead" or guess a value. No legacy error
aliases or generic string conversions are retained.

## 1. Verification is a gate, never a repair pass

Raw `Ir` records and successful arena lookups are not proof of validity. The
shared verifier currently always returns `IrError::NotImplemented`; this work
does not implement or bypass it. Before emission it must check:

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

Variadic signatures distinguish fixed parameters from the tail. Call-site
classification must validate tail types and apply the target/convention's
required promotions (including integer and floating promotions for supported C
varargs), aggregate rules, and any target-specific calling protocol. Do not
infer a callee signature from actual arguments or treat `variadic: true` as a
complete ABI implementation. Until those policies exist, reject unsupported
varargs/conventions/aggregate cases explicitly. The present IR has direct calls
by `FunctionId`, not an indirect-call instruction; do not invent one or a
runtime helper as a side effect of emission.

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
4. Record each **successor occurrence**, using `EdgeSite { source, successor }`:
   jump, then, else, each indexed switch case, and switch default. Store its
   target and positional arguments plus the eventual LLVM predecessor. A map
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

## Replaced files and preserved work

- `lowering/context.rs`: source scopes, defers and LLVM builder helpers replaced
  by module/function phase ownership and predeclaration sketches.
- `lowering/expressions.rs`, `statements.rs`: typed-AST walking replaced by SSA
  instruction/terminator APIs, edge identities and deferred PHI patching.
- `lowering/ir_lower.rs`: obsolete `IrProgram` consumer replaced by the verified
  input boundary. No `lower_ir` compatibility adapter remains.
- `lowering/llvm_lower.rs`: direct typed-AST lowering replaced by `emit_llvm` and
  the owned text artifact contract. No `lower` fallback remains.
- `lowering/types.rs`, `error.rs`: frontend/LLVM-dependent types and old errors
  replaced with target/type plans and explicit unfinished-operation errors.
- `lowering/abi.rs`: new shared ABI planning boundary.
- `lowering/mod.rs`, `backend/mod.rs`: expose the new contract without using a
  feature gate to conceal obsolete implementations.
- `lowering/test.rs`: old frontend/LLVM integration-like module tests replaced
  with small contract tests. Deleted repository integration tests stay deleted.
- **`lowering/next/ir_lower.rs` is preserved byte-for-byte.** This pre-existing,
  inactive prototype still references obsolete APIs and was already outside
  the module tree. It is not a fallback, compatibility implementation or part of
  the new architecture. Its owner must explicitly migrate it before activation.

Only this backend directory is changed. No driver wiring, frontend/IR changes,
manifest updates or changelog edits accompany this intentionally breaking
replacement. Current callers must use the new boundary when executable phases
are ready; the sketch does not claim to compile programs today.

## Validation and implementation checkpoints

Current tests cover explicit failure for empty/declaration-only inputs,
unimplemented verification/target resolution, distinct successor identities
and error diagnostics. They do not claim ABI, layout or LLVM correctness.

As phases are implemented, add focused tests for 32/64-bit pointer layouts and
alignment, aggregate/enum padding, recursive types, declaration/definition/call
ABI agreement, varargs, forward symbols and raw-bit constants. CFG tests must
include diamonds, loops, multiple results, same-target branch arms with
*different* arguments, repeated switch/default targets, and legalization that
splits the original predecessor. Negative tests must ensure malformed IDs,
ownership/dominance/type errors, unsupported semantics, unresolved fixups and
invalid LLVM cannot produce artifacts. Only then add executable backend tests;
do not resurrect removed legacy integrations to make this sketch look complete.
