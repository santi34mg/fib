# Owned SSA IR

See the [IR implementation contract](../../../../src/ir/README.md).

`Ir` owns stable indexed arenas; raw public records and display do not certify
validity. `FunctionBuilder` transactionally allocates blocks, values and slots,
requires termination and verifies before publishing. Failed/dropped builders leave
input unchanged.

`verify_ir` checks Bool/fixed-width integer SSA plus interpreter-owned byte constants: Constant/direct Call/comparison instructions,
Jump/Branch/Return/Unreachable, ownership, definitions, reachability/dominance,
entry and edge signatures, calls/returns, constants and orphan arena records.
Unreachable blocks are rejected. Storage, switches and other type/instruction
families fail explicitly; their records remain representation contracts. Byte values are
currently valid only for interpreted build-API calls and have no LLVM ABI lowering.

`ir::lowering::lower` produces verified SSA for single-file int32 helpers/calls,
initialized int32/bool locals, assignments, comparisons, if/else and returns.
Variable merges use block parameters rather than storage. Tests cover valid scalar graphs, invalid references,
ownership, signatures, forward uses, dominance and transactional construction.
