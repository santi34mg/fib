# `samples/`

Samples are executable language and backend regressions. Current examples cover
basic output, arithmetic/bitwise behavior, unsigned operations, enums, tagged
unions, nested aggregate assignment, pointers and heap structures, generics,
slices, builtin strings, tuple returns, and recursion.

Most samples import `std::libc`. Because that module contains a non-extern
pointer-dereferencing wrapper, most whole programs do not complete through the
flat-IR consumer and instead exercise direct typed-AST LLVM lowering.

## Missing End-to-End Coverage

- user-defined modules and imported non-extern functions;
- aliases/selective imports with link/run validation;
- float and NaN behavior;
- short-circuit side effects;
- nested defer and continue behavior;
- characters and Unicode;
- file, process, threading, synchronization, networking, and dynamic loading;
- release-mode runtime behavior;
- expected compile failures.

## Improvement Direction

Discover `samples/*.fib` automatically. Require each sample to have one policy:
exact output, compile-only, or expected failure. Keep expected output in one
machine-readable source rather than duplicating comments and test literals.
Record the expected lowering route until route parity is achieved.
