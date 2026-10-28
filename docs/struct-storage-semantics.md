# Struct values, storage and raw pointers

Struct typing, definite initialization, fixed-array value storage/assignment,
implicit readonly pointer views and warned one-layer pointer field access execute
through verified IR and LLVM, including multi-file calls and concrete generics.
See the [mutability and pointer rules](language-specification.md#values-initialization-and-mutability).
The [architecture](compiler-architecture.md) describes the current pipeline.

## Types and values

Declare a struct type with `comp`, then initialize every field by name:

```fib
comp Point: type = struct { x: int32, y: int32 };
fn shifted(point: Point) Point {
    result := point;
    result.x = result.x + 1;
    return result;
}
```

Structs are nominal: same-shaped definitions are distinct types, while aliases
retain the original identity. Fields may contain fixed-width numeric/bool values,
nested structs, or typed raw pointers to these types (including other pointers).
Empty structs and pointer-mediated recursive struct definitions work. By-value
recursive layouts are rejected. Arrays, slices, strings, opaque pointers,
function pointers and symbolic `usize` are outside this native storage subset.
Enum storage and struct/enum nesting also execute; see [enum semantics](enum-semantics.md).

Construction requires every field exactly once. Initializer expressions execute
in source order, even if fields are listed in a different order from their type
declaration. Storage field order follows the declaration. There are no default
field values, implicit numeric conversions or whole-struct equality operations.

Assignment, function arguments and returns copy the entire value. Mutating a
copy's fields does not change the original. Copying pointer fields copies their
addresses, so both values still refer to the same pointee. Internal struct/value
calls use the shared direct Fib fastcc ABI; this is not an external C struct ABI.
LLVM target data determines unpacked struct layout and field offsets.

## Initialization and places

Uninitialized local declarations allocate storage without initializing it.
Existing analysis requires every read to be definitely initialized. Individual
fields can be written and read independently; copying or returning the whole
struct requires every field to be initialized. No zero values are invented.

```fib
point: Point;
point.x = 10;
first := point.x;   // valid even before y is initialized
point.y = 20;
copy := point;      // now the whole struct can be read
```

Nested field assignments update the addressed field without loading uninitialized
siblings. Ordinary assignment evaluates the RHS before evaluating its destination.
Each destination pointer expression is evaluated once. Compound assignment retains
the current parser restriction to variable names; use `point.x = point.x + 1`.

Struct locals, uninitialized locals and address-taken scalar/pointer bindings use
function-owned stack slots. Other initialized scalars retain SSA values. Slots
are allocated in function entry; declaration initializers execute at the source
location, including on each loop iteration. Address-taken values are always read
from storage, so writes through aliases are visible to later direct reads.

Binding `const` and pointee `const` remain independent. Whole-binding/field writes
to an immutable struct are rejected. A `const` pointer binding can still mutate
its writable pointee; `const *T` prevents mutation through that pointer. The
unchecked intrinsic preserves these permissions.

## Confirmed array and readonly-view policy

Fixed arrays retain inline value semantics. Binding const prevents replacing the
whole array, while element const separately prevents element writes:

```fib
const arr: int[3] = [0, 1, 2];
arr[0] = 10;       // allowed: writable elements
arr = [1, 2, 3];   // rejected: immutable binding
```

`arr: const int[3]` instead allows whole-array replacement in the same storage
but forbids individual element writes. Slices use the same independent binding
and element permissions. Fixed-array values lower; runtime indexing and slices remain deferred.

Writable pointers and slices implicitly convert to readonly views of the
same underlying type; reverse conversion is forbidden. Nested pointee/element
types must match exactly, and casts cannot erase readonly permissions. Slice
compatibility is checked by analysis; slice execution remains deferred. It does not imply array-to-slice conversion or general borrowing safety.

## Checked pointer dereference

Postfix `value&` takes the address of a place; postfix `pointer*` dereferences it.
`(pointer*).x` explicitly dereferences before selecting a field. The shorthand
`pointer.x` inserts exactly one dereference and emits a nonfatal source-located
warning, preserving null checks and permissions. Two pointer layers require an
explicit dereference before the shorthand applies.
Null requires a pointer context; same-typed pointers support equality/inequality.

Ordinary dereference checks its pointer for null and traps before yielding a
place. This includes reads, writes, field projection and address-taking through
the dereference. Accessing a non-first field through null cannot bypass the check.
A trap is represented explicitly by a null comparison, branch and
`TrapReason::NullPointerDereference` in IR.

```fib
fn move(point: *Point) {
    (point*).x = (point*).x + 1;
}
```

Null checking is not allocation, lifetime, alignment, aliasing or initialization
validation. Non-null pointers must still refer to live, valid storage for the
access. No borrowing, escape or ownership checker is introduced. Source
initialization analysis does not infer initialization effects through arbitrary
pointer aliases or calls; initialize locals directly before relying on those
reads. Returning an address does not extend the lifetime of local storage.

## Explicit unchecked dereference

The compiler-provided `memory` namespace is available in normal and build-file
analysis, with ordinary declaration identities. Its name cannot also name a
planned source namespace. The place-returning intrinsic accepts one typed pointer:

```fib
pointer := point&;
value := std::memory::unchecked_deref(pointer).x;
std::memory::unchecked_deref(pointer).x = 42;
address := std::memory::unchecked_deref(pointer)&;
```

`std::memory::unchecked_deref(pointer)` evaluates the pointer once and yields its
pointee place without a null guard. It can be read, written, projected or addressed.
Type, arity, mutability and pointer-initialization checks still apply. A later
ordinary dereference, such as `std::memory::unchecked_deref(node).next*`, checks the
new pointer normally. Invalid unchecked accesses have no guaranteed trap behavior.

The intrinsic is not an ordinary runtime function or function-pointer value.
Compile-time aliases retain its identity; runtime storage, passing and indirect
invocation of the intrinsic itself are rejected. It emits memory IR operations,
not a native function call. General build-file memory interpretation remains
unsupported and fails explicitly.

## Verification and acceptance

IR verification checks struct/pointer references, by-value cycles, field
counts/indices/types, load/store signatures, slot ownership/membership,
reachability and dominance. These are structural guarantees, not a proof of raw
pointer memory validity or initialization. Loads/stores carry no implicit null
checks; source lowering supplies checks for ordinary dereferences. LLVM accesses
use conservative alignment and field GEPs without speculative `inbounds` promises.

[`samples/struct_storage.fib`](../samples/struct_storage.fib) compiles and returns
42. Tests cover copies, aliases, partial-field initialization, loops, recursive
calls/types, empty structs, source evaluation order, readonly rejection, malformed
IR, null traps and unchecked guard omission.
