# LLVM backend

See the [backend implementation contract](../../../../src/backend/README.md).

With LLVM enabled, `emit_llvm(ir, target)` verifies SSA and classifies each function
from its signature and resolved target. The ABI adapter uses that description
for declarations, incoming arguments, calls to the function, and returns from it.
There is no module-wide ABI setting or alternative public emission entry point.
C 32-bit integer/void entries and internal scalar Fib fastcc classification ship.
The latter supports Bool, fixed-width integers, IEEE floats, structs, enums and typed
pointers directly; full Fib ABI and
external C interoperability for the new scalars remain unfinished.

Emission supports multiple blocks/PHIs, backedges, scalar arithmetic/unary/casts,
comparisons, direct calls, jumps/branches/switches and returns/traps/unreachable.
Wrapping/IEEE semantics and guards for division, shifts and float-to-integer casts
follow [scalar semantics](../../../scalar-semantics.md). Entry is allocated first, instruction
emission uses reverse postorder, and PHIs are patched after all values exist.
Parallel branch arms targeting one block get separate trampoline predecessors.
LLVM verification precedes artifacts and PIC objects; driver linking uses `cc`.

LLVM identified unpacked structs, target-derived field GEPs, aggregate operations,
entry stack allocations and typed loads/stores are emitted. Pointer equality and
explicit null-check CFGs work; arbitrary accesses use conservative alignment.
Enums use target-derived tag-plus-union layout, typed payload operations and entry-materialized constants; see [enum semantics](../../../enum-semantics.md). Only generic CPU/default features are accepted. Broader ABI/layout, globals and
remaining instruction families are unfinished. Separate context/type/general
instruction planning sketches remain unused. Classification works without LLVM;
emission then reports an explicit feature error. `next/` is inactive legacy code.
