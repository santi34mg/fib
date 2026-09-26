# `std/`

## Status

The standard library is experimental. `std/libc.fib` is the part exercised most
often by samples; the rest of the directory has little or no compiler CI
coverage. Module presence does not imply correctness, portability, or runtime
testing.

## Cross-Cutting Constraints

- FFI declarations are handwritten for an assumed native libc ABI.
- The compiler cannot pass general native library/search/target flags.
- Opaque C types are sometimes allocated with fixed byte counts.
- There is no target-specific binding generation or platform gate.
- Manual memory management and raw pointers dominate APIs.

## Confirmed High-Risk Areas

- `atomic.fib` names ordinary loads/stores as atomic operations.
- thread/synchronization modules hard-code pthread object sizes and disagree on
  at least one size.
- stream readers use fixed buffers without complete growth/bounds protection.
- several libc declarations use suspicious return or parameter types.
- network functions include success-returning no-op implementations.
- JSON parsing is a placeholder.
- collection code copies one byte for generic-looking values and has underflow
  and lifecycle hazards.
- encoding validation is incomplete or stubbed.
- decimal file permission constants are used where octal modes are intended.

## Required Status Model

Create a generated or maintained matrix for every module with these fields:

- frontend-check status;
- native libraries/platforms required;
- implementation status: usable, partial, stub, unsafe, platform-specific;
- deterministic unit/runtime tests;
- ownership and allocation contract;
- known FFI/layout assumptions.

Do not expose placeholder APIs as successful operations. Prefer explicit
"unsupported" failures until the compiler can link and test their native
dependencies.
