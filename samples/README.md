# Samples

For one project that builds and runs with current capabilities, start with
[`example-project/`](../example-project/README.md). It covers scalar types, casts,
bit operations, calls, recursion and loops through an explicit build file.

All top-level `.fib` files in this directory use current tokenizer/parser syntax.
`frontend::parser::test::tests::samples_parse_with_current_frontend` discovers
and parses every top-level `.fib` sample, rejecting parse failures and empty ASTs.
This is **syntax regression coverage, not semantic or execution validation**.

Run the focused check from the repository root:

```sh
cargo test --no-default-features samples_parse_with_current_frontend
```

`builtin_scalars.fib`, `scalar_loops.fib`, `struct_storage.fib` and `enum_runtime.fib` also have
executable acceptance coverage and return 42. Compile them with `fibc build --single-file PATH`.
`indexed_globals.fib` also returns 42 and exercises embedded global data, borrowed
slice mutation and runtime array indexing. Its executable test uses no std sources.
Other samples retain intended/unverified output comments and broader unsupported
features. The `run` command and historical positional-file / `-I=std` commands
remain unsupported.
See the [parsing reference](../docs/parsing/README.md) for verified syntax and
[compiler status](../README.md) for pipeline boundaries.

## Dependencies and unresolved semantics

Samples retain qualified `libc::` calls without obsolete import declarations.
A future build plan must explicitly provide a source file named `libc`; parsing
these calls does not discover files or validate external signatures. The historical
standard-library bindings have been removed; replacement library implementations
and external C interoperability remain pending.

- `sorting.fib` retains parsed `T: type` parameters and type-valued arguments;
  generic analysis/specialization and pointer casts are not validated.
- `tagged_union.fib` retains payload values using parsed member calls such as
  `Token.Integer(7)`. Constructors now execute in the supported subset; this older sample still uses unported type bindings and external output.
- `linked_list.fib` and `memory_pool.fib` retain allocation, casts, indexed pointer
  stores, and defer calls. Pointer indexing, layout, allocation safety, and cleanup
  behavior need semantic/runtime tests. The linked-list allocation assumes a
  16-byte node layout, and the pool uses 64-bit integer addresses; these are
  illustrative target assumptions, not portable ABI guarantees.
- `slices.fib` illustrates view/mutation intent without asserting a runtime layout
  or implemented bounds checks.

Temporary parser restrictions are not intended language-design decisions. Future
analysis work is tracked in [GitHub issues](../docs/agents/issues/README.md).

## Example topics

- `builtin_scalars.fib`: executable wrapping, explicit widening and float casts.
- `scalar_loops.fib`: executable nested loops, scalar accumulation, breaks and continues.
- `enum_runtime.fib`: executable enum constructors, payload copies, switches and helper calls.
- `enum_payloads.fib`: direct payload types, unit normalization, Result(void, E), generic unit values and preserved call effects; exits 42.
- `struct_storage.fib`: executable struct copies, partial initialization, pointer mutation and unchecked places.

- `hello_world.fib`, `minimal.fib`: entry functions and output.
- `bitwise_ops.fib`, `unsigned_ops.fib`: arithmetic and bitwise operations.
- `enums.fib`, `switch_enum.fib`, `tagged_union.fib`: enum and pattern examples.
- `sorting.fib`, `slices.fib`: generic sorting and slice design examples.
- `string_functions.fib`, `multiple_returns.fib`: ordinary function declarations/calls and return shapes; string helper implementations are not provided.
- `nested_assign.fib`: nested field assignment.
- `linked_list.fib`, `memory_pool.fib`: allocation and cleanup design examples.
- `fib_bench.fib`: recursive Fibonacci benchmark intent, with 64-bit results
  sufficient for `fib(50)`; expensive to execute once a runner exists.

New syntax changes need positive and malformed-input parser tests. Keep sample
syntax covered by the discovery test; add real analysis and execution coverage
as those passes become usable rather than treating parsing as a conformance test.

`defer.fib` demonstrates reverse-order deferred blocks and has LLVM
compile/link/run coverage, returning 42.

`syscalls.fib` prints through `std::os::write` and handles a typed result. Use an
installed compiler or `build --sysroot library --single-file samples/syscalls.fib`.
The library's current OS implementation requires Linux x86-64.
