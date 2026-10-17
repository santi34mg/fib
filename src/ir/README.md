# SSA IR architecture sketch

The intended boundary is **`AnalyzedProject -> Ir -> backend`**. `Ir` is the
existing module-level object, extended with arenas; there is no parallel
program representation, resurrected typed AST, or fallback lowering path here.
The existing `driver::lower_to_ir(&AnalyzedProject) -> anyhow::Result<Ir>` remains
the lowering entry-point sketch. This directory does not implement it or add a
duplicate frontend-dependent entry point.

Only the data model, small lookup/display helpers, and structural unit tests
are implemented. **Construction and verification are not implemented.** Every
`FunctionBuilder` method and `verify_ir`, including verification of empty IR,
returns `IrError::NotImplemented { operation }`. Nothing here certifies an IR as
valid or executable. Public fields permit hand-authored, potentially invalid
records while the architecture is being developed.

## Backend-facing surface

All model types are re-exported from `crate::ir`; internal module filenames are
not part of the consumer contract.

- `Ir { functions, globals, types, constants, blocks, values, stack_slots }`.
  All fields are owned `Vec` arenas. `Ir::new()`, `Default`, `is_empty()`, and
  diagnostic `Display` are available. `is_empty` checks *all* arenas, not just
  blocks: external declarations are meaningful even without executable code.
- `FunctionId`, `GlobalId`, `TypeId`, `ConstantId`, `BlockId`, `ValueId`,
  `StackSlotId` are distinct index newtypes. Indices are **IR-wide**, stable
  within one IR, and never meaningful in another IR. Blocks, values and stack
  slots record `owner: FunctionId`. Names are owned diagnostic/linker names,
  never implicit symbol resolution. Lowering must translate canonical analyzed
  identities into these IDs and choose collision-free linker names.
- `Ir::{function, global, ty, constant, block, value, stack_slot}(id)` each
  returns `Option<&Record>` for its matching ID/record. Invalid indices return
  `None`; these are lookups, not validation. `operand_type(Operand)` returns
  `Option<TypeId>` from the value/constant record without validating that type.
- `Function { name, signature, linkage, body }` and `Signature { params,
  results, variadic, calling_convention }`. `FunctionBody::Declaration` retains
  external signatures without local entities. `Definition { entry, blocks,
  stack_slots }` refers into the shared arenas. Linkage is independent of body
  presence: an externally visible definition is not an external declaration.
- `BasicBlock { owner, params, instructions, terminator }`.
  `Instruction { results, kind }` is **non-terminating**. `Terminator` is
  `Jump`, `Branch`, `Switch`, `Return`, or `Unreachable`. `Edge { target, args }`
  carries positional incoming SSA arguments. Both branch successors and every
  switch/default successor are explicit, even when they target the same block.
- `Value { owner, ty, definition }`, with `ValueDefinition::BlockParameter`
  or `InstructionResult` locating its one definition. An instruction can have
  zero/one/multiple results. `Operand` is only `Value(ValueId)` or
  `Constant(ConstantId)`; types are recorded once in the corresponding arena.
- `IrType`, `Signature`, `Constant` and `ConstantKind` own all runtime data and
  depend only on IR IDs and standard library types. There are no frontend type,
  source lifetime, builtin-token or LLVM dependencies. Signedness belongs to
  integer operand types; comparison results are `Bool`. Pointer-sized integers
  stay symbolic until target lowering. Void returns use an empty results list.
- `Global { name, ty, mutable, linkage, initializer }` uses
  `GlobalInitializer::Declaration` or `Constant(ConstantId)`. A constant is
  immutable module-wide data, not a storage cell. Strings may use exact byte
  arrays; there is no implicit allocation, terminator byte or string-to-pointer
  conversion. Numeric constants retain raw bits, including floating point.

## SSA and memory contract (not yet checked)

1. A definition's block list owns every one of its blocks exactly once, includes
   entry, and agrees with owner metadata. Declarations own no local entities.
   Entry parameters match the function signature and receive call arguments;
   entry has no incoming CFG edges. Other parameters receive edge arguments.
2. Each `ValueId` occurs at exactly one definition. Its recorded location and
   type agree with the block parameter or instruction result. Uses refer to the
   same function, follow instruction ordering, and obey dominance. Loops and
   joins use block parameters and edge arguments, not reassignment or hidden phi
   instructions. No SSA construction/sealing algorithm has been selected here.
3. Every block ends with exactly one separate terminator. Block order never
   implies execution order. There are no label pseudo-instructions or implicit
   false edges. Edge targets stay in the same function; argument counts/types
   match target parameters. Returns and calls match declared signatures.
4. Stack slots are mutable storage records with function ownership, not SSA
   variables. `StackAddress`/`GlobalAddress` produce pointer values.
   `Address(Operand)` marks pointer operands for `Load`/`Store` and projections;
   it does not itself prove pointer typing, validity, bounds or mutability.
   Loads define fresh values; stores define none. Reading a constant/global
   address does not implicitly read memory. Memory effects occur in block
   instruction order; this sketch uses no memory-SSA token model.
5. Aggregate extract/insert operations operate on immutable values. Field/element
   address operations only calculate pointers; reading/writing through those
   pointers requires a separate load/store. Enum field extraction requires an
   established active variant. Any required checks must eventually be explicit
   control flow, not backend-invented language behavior.
6. Types/constants are self-contained and well formed. Value aggregates cannot
   have infinitely recursive layouts; pointer-mediated recursion may refer back
   to a type ID. Constants match their types, remain acyclic, and cannot reference
   function-local values. Switch cases are unique typed constants. Global
   initializers match their stored type. External symbols/signatures must agree.

These are requirements for the future verifier, **not claims that the current
records enforce them**. Consumers must not treat the presence of `Ir`, a
successful lookup, or a formatted dump as evidence that verification ran.

## Stub signatures

```rust,ignore
pub type IrResult<T> = Result<T, IrError>;
pub enum IrError { NotImplemented { operation: &'static str } }
pub fn verify_ir(ir: &Ir) -> IrResult<()>;

impl<'ir> FunctionBuilder<'ir> {
    pub fn new(ir: &'ir mut Ir, function: FunctionId) -> IrResult<Self>;
    pub fn create_block(&mut self, params: &[TypeId]) -> IrResult<BlockId>;
    pub fn block_params(&self, block: BlockId) -> IrResult<&[ValueId]>;
    pub fn create_stack_slot(
        &mut self, ty: TypeId, debug_name: Option<String>,
    ) -> IrResult<StackSlotId>;
    pub fn append_instruction(
        &mut self, block: BlockId, kind: InstructionKind, result_types: &[TypeId],
    ) -> IrResult<Vec<ValueId>>;
    pub fn set_terminator(&mut self, block: BlockId, terminator: Terminator) -> IrResult<()>;
    pub fn finish(self, entry: BlockId) -> IrResult<FunctionId>;
}
```

The builder will eventually stage incomplete blocks privately, allocate fresh
IDs, establish reciprocal definition/owner metadata and publish only terminated
blocks. There is deliberately no implemented ID allocation, name environment,
mutable-variable SSA conversion, graph repair, verifier or backend in this work.

## Replaced legacy bodies

- `mod.rs`: removed the removed-frontend imports, typed-AST lowering, partial
  constant lowering, `Temp`/`Label`/symbol operands and frontend-type predicates.
- `builder.rs`: replaced name scopes, mutable temporary allocation, defer/loop
  stacks and flat-stream block splitting with the fallible function API sketch.
- `expressions.rs`: replaced AST walking, implicit symbol loads and overwriting
  short-circuit temporaries with operand/value/constant records.
- `statements.rs`: replaced function/statement/assignment/defer/switch lowering
  (including incomplete legacy syntax) with block/edge/terminator records.
- `instruction.rs`: replaced frontend-dependent, flat control/memory operations
  with IR-owned non-terminating operations and explicit SSA result lists.
- `display.rs`: replaced obsolete formatting with an unverified diagnostic dump
  that does not dereference IDs and can therefore display malformed records.
- `test.rs`: replaced impossible frontend-lowering tests with structural model,
  accessor, display and explicit-not-implemented tests. No deleted integration
  tests are recreated. `types.rs` and `verify.rs` hold new architectural pieces.

The driver lowering boundary and LLVM adapter now sketch consumption of this
contract; neither executes. The interpreter remains unfinished. Compatibility
aliases for the legacy model would preserve the wrong semantics and are
intentionally absent. See [the pipeline overview](../../docs/compiler-architecture.md)
and [the backend plan](../backend/README.md) for the other boundaries.

## Decisions still needed before executable lowering

- Exact arithmetic overflow, shift, division-by-zero, floating comparison and
  cast semantics; trap/check representation and target restrictions.
- Aggregate/slice/enum layout, ABI legalization of multiple results, varargs,
  indirect calls, builtins/intrinsics and runtime helpers. Current direct calls
  identify functions by ID; no claim of complete language coverage is made.
- Pointer provenance, alignment, mutability, volatile/atomic access and memory
  validity policy. `Address` is a syntactic distinction, not a capability proof.
- Type interning/canonicalization, richer verifier errors, source/debug mapping,
  arena editing/remapping, and how callers will distinguish verified IR later.

These questions do not block the sketch. They must be resolved explicitly
before a backend can soundly execute this representation.
