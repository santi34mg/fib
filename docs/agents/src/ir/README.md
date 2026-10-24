# Owned SSA IR

See the [IR implementation contract](../../../../src/ir/README.md).

`Ir` owns stable indexed arenas; raw public records and display do not certify
validity. `FunctionBuilder` transactionally allocates blocks, values and slots,
requires termination and verifies before publishing. Failed/dropped builders leave
input unchanged.

`verify_ir` checks Bool/fixed-width integer/IEEE float SSA plus interpreter-owned
byte constants: Constant/direct Call/arithmetic/comparison/unary/numeric Cast
instructions, Jump/Branch/Return/Trap/Unreachable, ownership, definitions, reachability/dominance,
entry and edge signatures, calls/returns, constants and orphan arena records.
Unreachable blocks are rejected. Structs, typed pointers, null constants,
stack-slot ownership, aggregate operations and typed memory operations are
verified. Globals, switches and other unsupported families fail explicitly; their records remain representation contracts. Byte values are
currently valid only for interpreted build-API calls and have no LLVM ABI lowering.

`ir::lowering::lower` produces verified SSA for fixed-width integer/float/bool
helpers/calls, initialized locals, assignments, arithmetic/bitwise/shifts,
comparisons, explicit casts, short-circuit booleans, if/else, while loops, break/continue and returns.
See [scalar semantics](../../../scalar-semantics.md) for operation rules.
SSA variable merges and loop-carried values use block parameters. Struct,
uninitialized and address-taken locals use explicit stack storage; ordinary
dereference adds a null-check/trap CFG, and the unchecked intrinsic omits it. Tests cover valid scalar graphs, invalid references,
ownership, signatures, forward uses, dominance and transactional construction.
