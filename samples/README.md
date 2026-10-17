# Samples

All 15 `.fib` files in this directory use current tokenizer/parser syntax.
`frontend::parser::test::tests::samples_parse_with_current_frontend` discovers
and parses every top-level `.fib` sample, rejecting parse failures and empty ASTs.
This is **syntax regression coverage, not semantic or execution validation**.

Run the focused check from the repository root:

```sh
cargo test --no-default-features samples_parse_with_current_frontend
```

The CLI's build/run commands, analysis, and lowering remain unfinished. Intended
output comments describe example goals, not verified results. There is no working
e2e runner, and historical positional-file / `-I=std` commands are unsupported.
See the [parsing reference](../docs/parsing/README.md) for verified syntax and
[compiler status](../README.md) for pipeline boundaries.

## Dependencies and unresolved semantics

Samples retain qualified `libc::` calls without obsolete import declarations.
A future build plan must explicitly provide a source file named `libc`; parsing
these calls does not discover files or validate external signatures. Historical
bindings in `std/libc.fib` have not been migrated as part of this sample change
and are not a working dependency today.

- `sorting.fib` retains parsed `T: type` parameters and type-valued arguments;
  generic analysis/specialization and pointer casts are not validated.
- `tagged_union.fib` retains payload values using parsed member calls such as
  `Token.Integer(7)`. This is not a claim of implemented enum constructors.
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

- `hello_world.fib`, `minimal.fib`: entry functions and output.
- `bitwise_ops.fib`, `unsigned_ops.fib`: arithmetic and bitwise operations.
- `enums.fib`, `switch_enum.fib`, `tagged_union.fib`: enum and pattern examples.
- `sorting.fib`, `slices.fib`: generic sorting and slice design examples.
- `string_builtins.fib`, `multiple_returns.fib`: builtin calls and return shapes.
- `nested_assign.fib`: nested field assignment.
- `linked_list.fib`, `memory_pool.fib`: allocation and cleanup design examples.
- `fib_bench.fib`: recursive Fibonacci benchmark intent, with 64-bit results
  sufficient for `fib(50)`; expensive to execute once a runner exists.

New syntax changes need positive and malformed-input parser tests. Keep sample
syntax covered by the discovery test; add real analysis and execution coverage
as those passes become usable rather than treating parsing as a conformance test.
