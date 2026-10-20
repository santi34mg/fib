# Owned SSA IR

The executable boundary is `AnalyzedProject -> Ir -> backend`. Arena records
remain public and unverified until `verify_ir` succeeds. IDs are stable indices
within one IR; the IR has no frontend or LLVM dependencies.

## Working construction and verification

`FunctionBuilder` starts from a declaration and stages a cloned IR transaction.
It allocates IR-wide block, value and stack-slot IDs, records reciprocal owner
and definition locations, rejects insertion after termination and repeated
terminators, and requires an explicit entry and every block to be terminated.
`finish` verifies the complete staged IR before publishing it. Dropping the
builder or failing any finish check leaves the original IR unchanged. Cloning
is an initial correctness choice, not an optimized construction strategy.

`verify_ir` supports Bool and fixed-width integers (1..=128), typed bool/integer
constants, declarations and definitions, block parameters, Constant/direct Call/integer comparison instructions, Jump/Branch/Return/Unreachable terminators. It checks:

- References, unique symbol names, arena membership and function ownership.
- Unique SSA definitions and reciprocal definition positions.
- Entry/function signatures; no edge may target entry.
- Reachability, instruction ordering and dominance, including cyclic CFGs.
- Branch conditions, edge arguments, instruction results, call arguments and
  return signatures.
- Integer widths and raw constant bits; orphan values and blocks are errors.

Empty IR is valid and actually checked. Unsupported types, constants, storage,
switches and instruction families fail explicitly. Stack-slot allocation exists,
but verification of storage is unfinished, so a builder containing storage cannot
publish a body. Unreachable blocks are rejected rather than assigned arbitrary
values/dominance. `IrError::Invalid` identifies malformed supported IR;
`NotImplemented` identifies unsupported verification families.

## Representation contracts and remaining work

`Ir` owns functions, globals, types, constants, blocks, values and stack slots.
`FunctionBody::Declaration` owns no local records; `Definition` lists entry,
blocks and stack slots. `ValueDefinition` records one block parameter or
instruction result. Block parameters receive positional edge arguments, with
entry parameters instead receiving caller arguments. No arena order implies
fallthrough. Values are immutable; mutable storage requires explicit addresses,
loads and stores. `Display` remains a diagnostic dump marked unverified.

The broader records describe future support, not executable guarantees:
pointers, arrays/slices, aggregates, enums, float/raw-byte constants, globals,
casts, arithmetic, memory operations and switch validation remain unfinished.
Unit/never/character/string runtime representation, indirect calls, generic body
specialization and general source lowering still need implementation.

The driver lowers a single-file scalar subset: int32 helpers/direct calls,
initialized int32/bool locals, assignment, comparisons, conditionals and returns.
Branch joins use block parameters; mutable local bindings need no storage.
See `../backend/README.md` for ABI restrictions and executable tests.

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

## Broader SSA and memory requirements

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

The scalar subset above is checked today; unsupported families must gain these
checks before becoming executable. Public records alone do not enforce them. Consumers must not treat the presence of `Ir`, a
successful lookup, or a formatted dump as evidence that verification ran.


## Decisions still needed for broader executable lowering

- Exact arithmetic overflow, shift, division-by-zero, floating comparison and
  cast semantics; trap/check representation and target restrictions.
- Aggregate/slice/enum layout, ABI legalization of multiple results, varargs,
  indirect calls, builtins/intrinsics and runtime helpers. Current direct calls
  identify functions by ID; no claim of complete language coverage is made.
- Pointer provenance, alignment, mutability, volatile/atomic access and memory
  validity policy. `Address` is a syntactic distinction, not a capability proof.
- Type interning/canonicalization, richer verifier errors, source/debug mapping,
  arena editing/remapping, and how callers will distinguish verified IR later.

These questions must be resolved before executing the corresponding operations;
they do not block the supported scalar/conditional subset.
