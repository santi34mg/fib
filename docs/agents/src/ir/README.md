# Owned SSA IR

The [language specification](../../../language-specification.md) is implemented for prelude names, casts, shallow readonly views,
warned pointer field access and exhaustive switches. Concrete generic functions,
multiple source files and fixed-array values execute. Package-rooted lookup, scoped
visibility, installed std sources and Linux x86-64 syscalls execute. Recursive source
discovery remains deferred. Checked runtime indexing/slicing and constant-initialized
globals execute.

See the [IR implementation contract](../../../../crates/fib-ir/src/ir/README.md).

`Ir` owns stable indexed arenas; raw public records and display do not certify
validity. `FunctionBuilder` transactionally allocates blocks, values and slots,
requires termination and verifies before publishing. Failed/dropped builders leave
input unchanged.

`verify_ir` allocates program-indexed predecessor/dominator workspace once per
verification, releasing each function's CFG sets after its checks. It retains
whole-program ownership checks and function-level dominance/active-variant proofs.

`verify_ir` checks Bool/fixed-width integer/IEEE float SSA plus interpreter-owned
byte constants: Constant/direct Call/arithmetic/comparison/unary/numeric Cast
instructions, Jump/Branch/Switch/Return/Trap/Unreachable, ownership, definitions, reachability/dominance,
entry and edge signatures, calls/returns, constants and orphan arena records.
Unreachable blocks are rejected. Structs, typed pointers, null constants,
stack-slot ownership, aggregate operations and typed memory operations are
verified. Enum construction, tags, constants and payload extraction are checked, including active-variant proofs. Globals and constant address/slice projections are verified, including known array extents and constant-graph cycles. Byte values are
currently valid only for interpreted build-API calls and have no LLVM ABI lowering.

`ir::lowering::lower` produces verified SSA for fixed-width integer/float/bool
helpers/calls, initialized locals, assignments, arithmetic/bitwise/shifts,
comparisons, explicit casts, short-circuit booleans, if/else, while loops, break/continue and returns.
Defer statements and blocks register at their source position and execute in
reverse order on lexical scope exits. Return expressions are evaluated before
cleanup; break/continue clean up only scopes left by that loop transfer. Cleanup
uses ordinary verified CFG and memory operations; traps do not unwind it.
See [scalar semantics](../../../scalar-semantics.md) for operation rules.
Enum constructors and switches with copied payload bindings also lower; see [enum semantics](../../../enum-semantics.md). SSA variable merges and loop-carried values use block parameters. Struct,
uninitialized and address-taken locals use explicit stack storage; ordinary
dereference adds a null-check/trap CFG, and the unchecked intrinsic omits it. Tests cover valid scalar graphs, invalid references,
ownership, signatures, forward uses, dominance and transactional construction.

Concrete generic instances are keyed by canonical template/type identities, with
rechecked binding environments used for storage classification and SSA joins.
Type parameters/arguments are absent from runtime signatures/calls. Multi-file
function identities are registered before bodies; internal symbols distinguish
same-spelled declarations. Fixed-array aggregates validate lengths, element
signatures and by-value cycles. Numeric casts include explicit bool conversions.

Non-unit enum constructors carry one operand of the actual payload type; switch
bindings extract that operand under the existing active-variant proof. Normalized
unit variants have no stored payload. Their bindings synthesize an empty tuple
value, also used for ordinary runtime void values. Void-returning calls retain
effects before yielding unit in value positions, and unit equality evaluates both
operands. Function void returns have no result; opaque pointers retain opaque
pointees rather than the unit-value representation.

## Standard-library operations

Memory intrinsics resolve under `std::memory`. Source lowering supports
`slice_from_parts`, `slice_data`, indexing, indexed assignment, slicing and `.len`.
Bounds guards use `IndexOutOfBounds`/`InvalidSlice` traps before narrowing offsets;
element access also preserves null checks. Array temporaries use function-owned
slots, while slices of places retain original storage. `Syscall` takes a uint64 number
and up to six uint64/pointer arguments and returns int64. Verification checks
types, signatures, ownership and dominance. LLVM emits Linux x86-64 inline
assembly with side effects and register/flags/memory clobbers; other targets fail.
Package-private typed pointer/word helpers lower to verified uint64-to-pointer
and pointer-to-uint64 casts, supporting source std mappings and allocation.
Build-file interpretation never executes syscalls.

File-level storage has constant initial data, including relocations and slice
views. Lowering registers all global identities before initializers or bodies.
No startup function is generated; the build-file interpreter rejects global accesses
before applying operations. Value-returning `comp fn` evaluation is scaffolding
with an explicit diagnostic, never a runtime fallback.
