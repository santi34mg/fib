# `src/backend/lowering/`

## Files and Ownership

- `context.rs`: per-function direct-lowering state and LLVM helpers.
- `error.rs`: backend `LowerError`.
- `types.rs`: Fib type to LLVM type mapping and manual layout calculations.
- `llvm_lower.rs`: direct `TypedProgram -> LLVM` declarations/functions.
- `expressions.rs`: direct expression, lvalue, aggregate, bounds, and builtin lowering.
- `statements.rs`: direct statements, control flow, defer, and switch lowering.
- `ir_lower.rs`: `IrProgram -> LLVM` consumer.
- `test.rs`: backend unit and IR-text assertions.

## Route Selection

The driver first tries `TypedProgram -> IrProgram -> LLVM`. On any failure it
retries `TypedProgram -> LLVM`. The direct route is the complete route today;
most samples fall back because imported `std::libc` includes pointer operations
unsupported by the IR consumer.

## Direct-Lowering State

`FunctionLowering` owns the LLVM context/module/builder, current function,
source-name-to-alloca map, symbol table, defer stack, loop targets, and bounds
check policy.

This centralized state is useful, but ordinary blocks do not save/restore the
alloca map or matching symbol-table scopes. Shadowed branch/loop locals can
replace outer storage during later lowering. Switch payloads have bespoke
restoration, which should not remain the only scoped case.

## Type and ABI Model

Current mappings include:

- bool to `i1`;
- integers through 128-bit LLVM integers;
- `@usize` to `i64`;
- floats through `fp128`;
- pointers, strings, and function pointers to opaque `ptr`;
- slices to `{ ptr, i64 }`;
- structs/tuples to literal LLVM structs;
- plain enums to `i32`;
- payload enums to `{ i32, [N x i64] }`.

No target triple/data layout is set. Manual sizing assumes 64-bit pointers and
can under-align payloads wider than eight bytes. Native libc ABI and POSIX-like
functions are assumed by bounds checks and string builtins.

## Bounds Checks

Direct lowering emits runtime checks for dynamic index and slice bounds unless
release mode is enabled. Constant bounds are checked by the frontend in every
mode. Runtime failure reports through libc and aborts. The IR route does not
currently carry indexed programs to successful LLVM emission.

## Known Correctness Risks

- no LLVM module verification in either route;
- direct local storage is not lexically scoped under shadowing;
- flat-IR short-circuit output violates SSA dominance;
- flat-IR pointer operations may panic on unchecked value conversion;
- calls can auto-declare functions before later definitions, causing duplicate
  or renamed LLVM symbols without a declaration-first pass;
- statements after return can be emitted after a terminator;
- non-void fallthrough is sealed with `unreachable` instead of rejected;
- slices can point into local or temporary stack arrays and escape;
- tagged-union layout is not target-aware;
- C variadic default promotions are not performed;
- float `!=` uses ordered-not-equal, making NaN behavior potentially surprising;
- builder failures are sometimes ignored or printed directly;
- aggregate-heavy code uses many current-block allocas and spill/reload cycles;
- module constants are incomplete and declaration-order dependent.

## Verification and Tests

Every successful route should call LLVM verification before printing IR. Tests
should be able to force each route, verify the module, link it, run it, and
compare observable behavior for the shared subset.

High-value regressions include short-circuit side effects, pointer equality,
branch/loop/switch shadowing, statements after return, nested defer, forward
calls, local-array slice escape, wide tagged-union payloads, NaN comparisons,
and variadic narrow/float arguments.
