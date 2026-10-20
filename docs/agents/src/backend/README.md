# LLVM backend

See the [backend implementation contract](../../../../src/backend/README.md).

With LLVM enabled, `emit_llvm(ir, target)` verifies SSA and classifies each function
from its signature and resolved target. The ABI adapter uses that description
for declarations, incoming arguments, calls to the function, and returns from it.
There is no module-wide ABI setting or alternative public emission entry point.
Only C 32-bit integer/void classification ships; Fib ABI fails explicitly.

Emission supports multiple blocks/PHIs, backedges, comparisons, direct calls,
jumps/branches and returns/unreachable. Entry is allocated first, instruction
emission uses reverse postorder, and PHIs are patched after all values exist.
Parallel branch arms targeting one block get separate trampoline predecessors.
LLVM verification precedes artifacts and PIC objects; driver linking uses `cc`.

Only generic CPU/default features are accepted. Broader ABI/layout, storage and
remaining instruction families are unfinished. Separate context/type/general
instruction planning sketches remain unused. Classification works without LLVM;
emission then reports an explicit feature error. `next/` is inactive legacy code.
