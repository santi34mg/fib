# LLVM backend

The [language specification](../../../language-specification.md) is implemented for prelude names, casts, shallow readonly views,
warned pointer field access and exhaustive switches. Concrete generic functions,
multiple source files and fixed-array values execute. Package-rooted lookup, scoped
visibility, installed std sources and Linux x86-64 syscalls execute. Recursive source
discovery remains deferred. Checked runtime indexing/slicing and constant-initialized
globals execute.

See the [backend implementation contract](../../../../crates/fib-backend/src/backend/README.md).

Private std typed pointer/word conversions now emit verified `ptrtoint`/`inttoptr`
operations for the Linux x86-64 source allocator. Allocation itself remains Fib
source using `sbrk`, rather than a backend-owned runtime shim.

With LLVM enabled, `emit_llvm(ir, target)` verifies SSA and classifies each function
from its signature and resolved target. The ABI adapter uses that description
for declarations, incoming arguments, calls to the function, and returns from it.
There is no module-wide ABI setting or alternative public emission entry point.
Linux x86-64 System V C classification and internal Fib fastcc classification ship.
Extended main signatures use an analyzed source wrapper with C argc/argv/envp
parameters and int32 result; source main retains Fib ABI. Process adaptation is
implemented by the driver and source std, using the same emission path.
Fib supports Bool, fixed-width integers, IEEE floats, structs, enums and typed
pointers directly. `callconv "C"` supports integer/bool, float32/64, pointers,
function pointers and nonempty structs, including register coercions and indirect
struct boundaries. Unsupported C types/targets are rejected; the full production
Fib ABI remains unfinished. Function addresses and indirect calls execute,
including callbacks stored in structs, preserving their signature's convention.

Emission supports multiple blocks/PHIs, backedges, scalar arithmetic/unary/casts,
comparisons, direct/indirect calls, jumps/branches/switches and returns/traps/unreachable.
Wrapping/IEEE semantics and guards for division, shifts and float-to-integer casts
follow [scalar semantics](../../../scalar-semantics.md). Entry is allocated first, instruction
emission uses reverse postorder, and PHIs are patched after all values exist.
Parallel branch arms targeting one block get separate trampoline predecessors.
LLVM verification precedes artifacts and PIC objects; driver linking uses `cc`.

LLVM identified unpacked structs, target-derived field GEPs, aggregate operations,
entry stack allocations and typed loads/stores are emitted. Pointer equality and
explicit null-check CFGs work; arbitrary accesses use conservative alignment.
Enums use target-derived tag-plus-union layout, typed payload operations and entry-materialized values; see [enum semantics](../../../enum-semantics.md). Only generic CPU/default features are accepted. Broader ABI/layout and
remaining instruction families are unfinished. Classification works without LLVM;
emission then reports an explicit feature error.

Direct enum payloads use the existing typed union storage without an implicit
struct wrapper. Normalized unit variants store only their tag. Runtime void values
use empty aggregates through the internal Fib ABI, while void returns remain
resultless and opaque `*void` remains unsized. Function-pointer null checks use the
same pointer comparison emission as data pointers.

Fixed arrays use a private named wrapper containing an LLVM array; completion
waits for the element layout and uses target-derived storage/alignment. Aggregate
construction, copies and the shared internal Fib ABI preserve inline values.
`usize` IR uses pointer-width integers resolved through target data. Array/slice
indexing and slicing emit explicit bounds guards and ordinary element GEPs without
`inbounds` promises.

`llvm_data` declares globals before evaluating their constant initializers. Physical
constant layouts preserve typed enum payloads and pointer relocations, validating
size, alignment and field/array offsets against runtime layouts. Runtime uses
materialize these constants into canonical SSA types when needed. Global initial
data is embedded in the object; no startup constructor is emitted. Binding `const`
does not alone imply physically readonly storage because elements may be writable.
