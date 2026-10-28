# Executable enums

Construction, payload copies, native execution and [exhaustive switches](language-specification.md#enums-and-switches)
are implemented. Coverage is validated independently of reachability and rechecked
by annotation validation.

Enum types are nominal. Variants use declaration-order, zero-based discriminants.
A payloadless variant is a value (`E.Empty`); a variant declared with payload
fields is a constructor (`E.Number(42)`). An empty payload still requires a call
(`E.Unit()`). Constructor arguments evaluate once, from left to right. Compile-time
aliases of the enum type preserve its identity and constructors.

Enums support initialized locals, whole-value assignment, addresses, typed pointer
loads/stores, struct fields, other enum payloads, and direct internal Fib helper
arguments/results. Pointer-mediated recursive types work; infinite by-value
layouts and unsupported payload representations are rejected. The C entry ABI
remains `int32`/void; this is not external C enum interoperability.

## Switches and payload copies

`switch (subject) { when .Number(payload) { ... } when else { ... } };`
evaluates the subject once. A selected payload binding is a mutable copy of the
variant's payload struct. Changing its fields leaves the original enum unchanged;
following a pointer contained in it still reaches the original pointed-to storage.
The binding is scoped to its arm. Duplicate variants and arms after a wildcard
are rejected by analysis.

The language requires exhaustive switches: explicitly cover every
variant or use `when else` for all remaining variants. Missing coverage is a
compile-time error; there is no implicit unmatched fallthrough. A selected arm
may still complete normally and continue after the switch. Return, break and continue retain their ordinary
meaning; break/continue target the enclosing loop. A switch does not introduce a
loop. An empty enum has no constructed values, and switching over one is exhaustive.

## Verified IR

An enum type records variant discriminants and payload field types. Source lowering
uses one payload-struct field per payload variant, or no fields for a payloadless
variant. Construction, tag extraction and payload extraction use `Enum`, `EnumTag`
and `EnumField`; dispatch uses the integer `Switch` terminator. Source tag values
use `uint32`, while raw IR permits discriminants through `u128::MAX`.

Verification checks enum signatures, discriminant uniqueness, finite layouts,
constant graphs, tag capacity and switch cases/edges. Payload extraction requires
a proof of the active variant: a matching construction/constant, or a dominating
dedicated case successor switching on `EnumTag` of the same SSA enum value.
A switch successor shared with another case or default does not establish that
proof. Lowering copies bound payloads to arm-local stack storage and merges
continuing SSA values using block parameters.

## Native representation

LLVM uses an identified unpacked struct with a tag and shared payload union.
The tag uses the smallest unsigned width among 8/16/32/64/128 bits that holds all
discriminants; empty enums use 8 bits. Payloadless enums have no union field.

LLVM target data determines every payload's allocation size and ABI alignment.
The union carrier contains a zero-length array of the most-aligned payload type
and a byte array sized for the largest payload. LLVM supplies trailing padding.
This makes alignment independent of which variant is largest. Payloads exceeding
the supported LLVM array length fail explicitly. Non-integral pointer layouts
are rejected because byte-based union storage cannot preserve their extra state.

Construction initializes inactive storage and writes the active payload through
its typed representation. Extraction uses the verified active variant's typed
fields. Scratch allocations live in the function entry and are reused across
loop iterations. Aggregate and enum constants, including forward references, are
materialized in dependency order at function entry. Native enum values pass
directly through the existing internal Fib fastcc ABI; layout is an internal
implementation contract, not a stable external ABI.

The executable sample is [`enum_runtime.fib`](../samples/enum_runtime.fib).
Runtime indexing/slices, runtime globals and enum-to-integer
casts are outside this lowering extension.
