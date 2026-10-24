# Owned SSA IR

The [language specification](../../../language-specification.md) is implemented for prelude names, casts, shallow readonly views,
warned pointer field access and exhaustive switches. Concrete generic functions,
multiple source files and fixed-array values execute. Package metadata/scoped
visibility exist; package-rooted lookup/discovery and runtime indexing/slices remain deferred.

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
