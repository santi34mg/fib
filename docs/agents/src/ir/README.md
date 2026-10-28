# Owned SSA IR

The [language specification](../../../language-specification.md) is implemented for prelude names, casts, shallow readonly views,
warned pointer field access and exhaustive switches. Concrete generic functions,
multiple source files and fixed-array values execute. Package-rooted lookup, scoped
visibility, installed std sources and Linux x86-64 syscalls execute. Recursive source
discovery and general runtime indexing/slices remain deferred.

See the [IR implementation contract](../../../../src/ir/README.md).

`Ir` owns stable indexed arenas; raw public records and display do not certify
validity. `FunctionBuilder` transactionally allocates blocks, values and slots,
requires termination and verifies before publishing. Failed/dropped builders leave
input unchanged.

`verify_ir` checks Bool/fixed-width integer/IEEE float SSA plus interpreter-owned
byte constants: Constant/direct Call/arithmetic/comparison/unary/numeric Cast
instructions, Jump/Branch/Switch/Return/Trap/Unreachable, ownership, definitions, reachability/dominance,
entry and edge signatures, calls/returns, constants and orphan arena records.
Unreachable blocks are rejected. Structs, typed pointers, null constants,
stack-slot ownership, aggregate operations and typed memory operations are
verified. Enum construction, tags, constants and payload extraction are checked, including active-variant proofs. Globals and other unsupported families fail explicitly; their records remain representation contracts. Byte values are
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

## Standard-library operations

Memory intrinsics resolve under `std::memory`. Source lowering supports
`slice_from_parts`, `slice_data` and slice `.len`, preserving the existing
limitations on general runtime indexing/slicing. `Syscall` takes a uint64 number
and up to six uint64/pointer arguments and returns int64. Verification checks
types, signatures, ownership and dominance. LLVM emits Linux x86-64 inline
assembly with side effects and register/flags/memory clobbers; other targets fail.
The package-private mmap helper lowers to a verified uint64-to-pointer cast.
Build-file interpretation never executes syscalls.
