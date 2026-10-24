# Current Fib capabilities

This is one runnable project using the compiler's current executable subset.
`main.fib` and `features.fib` run nine demonstrations and exit with **0** when
all checks pass.
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

The build file explicitly registers `main.fib` and `features.fib`, selects `main`
as entry, and chooses `demo` beside the build file as output. Source and output
paths are relative to the build file. From another directory, pass the appropriate
path to `build.fib`. Both source files are required.

## What the program demonstrates

| Exit on failure | Demonstration |
| --- | --- |
| 1 | All signed/unsigned integer widths (8, 16, 32, 64, 128), explicit widening/narrowing, sign conversion, wrapping arithmetic and signed right shift |
| 2 | All four float widths (16, 32, 64, 128), arithmetic, float/integer casts, truncation, infinity, NaN and signed zero |
| 3 | Packed IPv4 arithmetic, bitwise AND/OR/XOR/complement, shifts, mutable locals, compound assignment and a population-count loop |
| 4 | Dynamic and unconditional loops, zero iterations, nested if/else, scoped locals, break and continue |
| 5 | Typed helpers, forward calls, recursion, comparisons and early returns |
| 6 | Boolean short-circuit AND/OR guarding integer division/remainder by zero |
| 7 | Struct field initialization, whole-value copies, pointer mutation and shallow readonly pointer views |
| 8 | Payload enum constructors, enum copies and internal calls, exhaustive switches and independent switch payload copies |
| 9 | Generic scalar/array specialization, fixed-array literals, copies, assignment, calls/returns and `.len`, numeric/bool conversions and independently typed multi-bindings with ordered initialization |

`main` also demonstrates `const`, the 32-bit `int`/`uint` aliases, inferred
bindings, a void helper and the required `main() int32` entry contract. It calls
the three `pub(root)` checks in `features.fib` using file-qualified names such as
`features::structs_and_pointers()`. Their types and helper functions remain
file-private. Generic functions receive ordinary `type` parameters and concrete
type arguments. Typed numeric values never widen implicitly.
Invalid runtime integer division, shifts and float-to-integer
conversions trap; this example guards or avoids those invalid operations.

## Boundaries

Multi-file calls, structs, enums, pointers, concrete generic specialization and
fixed-array values execute through verified SSA and LLVM. Fixed arrays support
whole-value operations and `.len`; runtime indexing and slices remain deferred.
The example uses explicit checked pointer dereferences, so it does not trigger
the warning for implicit pointer field access.

Console I/O/runtime strings, runtime globals, defer and indirect calls remain
unsupported by executable lowering. Build-file interpretation supports the
literal source/entry/output operations used here; general control flow and helper
execution remain unfinished. Automatic discovery and the CLI `run` and `test`
commands are not implemented. See the [implementation guide](../docs/agents/README.md)
for current boundaries and [scalar semantics](../docs/scalar-semantics.md) for
numeric rules.

The executable acceptance suite copies the build file and both source files into a temporary
project, builds through the CLI and checks exit 0 with a timeout. Frontend/IR
tests additionally analyze and verify this project without requiring LLVM.
