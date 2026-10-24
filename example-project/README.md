# Current Fib capabilities

This is one runnable project using the compiler's current executable subset.
`main.fib` runs six demonstrations and exits with **0** when all checks pass.
It prints nothing: a console I/O API and external libc calls are not currently
part of the supported executable subset.

## Build and run

From the repository root, using Rust 1.98.0, LLVM 21 and host `cc`:

```sh
cargo run -- build --build-file example-project/build.fib
./example-project/demo
echo $?  # expected: 0
```

If LLVM discovery needs an explicit prefix, set `LLVM_SYS_211_PREFIX` to your
LLVM 21 installation directory (for example `/usr/lib/llvm/21`) for the Cargo
command. Add `--offline` before `--` when using cached dependencies offline.
The commands above use a POSIX shell. The build file selects the exact output
name `demo`.

With `fibc` already installed:

```sh
fibc build --build-file example-project/build.fib
./example-project/demo
```

The build file registers exactly one source, selects its logical name as entry,
and chooses `demo` beside the build file as output. Paths are relative to the
build file, so this works regardless of the invoking directory. Pass an explicit
`--build-file`: automatic discovery and the `run` command are not implemented.

You can also compile the same program without evaluating the build file:

```sh
cargo run -- build --single-file example-project/main.fib
./example-project/main
echo $?  # expected: 0
```

## What the program demonstrates

| Exit on failure | Demonstration |
| --- | --- |
| 1 | All signed/unsigned integer widths (8, 16, 32, 64, 128), explicit widening/narrowing, sign conversion, wrapping arithmetic and signed right shift |
| 2 | All four float widths (16, 32, 64, 128), arithmetic, float/integer casts, truncation, infinity, NaN and signed zero |
| 3 | Packed IPv4 arithmetic, bitwise AND/OR/XOR/complement, shifts, mutable locals, compound assignment and a population-count loop |
| 4 | Dynamic and unconditional loops, zero iterations, nested if/else, scoped locals, break and continue |
| 5 | Typed helpers, forward calls, recursion, comparisons and early returns |
| 6 | Boolean short-circuit AND/OR guarding integer division/remainder by zero |

`main` also demonstrates `const`, the 32-bit `int`/`uint` aliases, inferred
bindings, a void helper and the required `main() int32` entry contract. Functions
and calls use explicit scalar signatures; typed numeric values never widen
implicitly. Invalid runtime integer division, shifts and float-to-integer
conversions trap; this example guards or avoids those invalid operations.

## Boundaries

The project deliberately uses one source file plus `build.fib`. Multi-file plans
can be constructed and analyzed, but executable lowering still requires one
source unit. Build-file interpretation supports the literal source/entry/output
operations used here, rather than general loops or helper execution.

Semantic analysis supports more than executable lowering. This runnable project
does not use runtime aggregates, arrays/slices/pointers, globals, strings, generic
bodies, switch, defer, uninitialized declarations or indirect calls. See
[compiler architecture](../docs/compiler-architecture.md) for the complete
analysis/runtime boundary and [scalar semantics](../docs/scalar-semantics.md) for
numeric rules.

The executable acceptance suite copies both source files into a temporary
project, builds through the CLI and checks exit 0 with a timeout. Frontend/IR
tests additionally analyze and verify this project without requiring LLVM.
