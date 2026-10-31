# Samples

Every top-level `.fib` sample builds and runs with the current compiler and source
standard library on Linux x86-64. From the repository root:

```sh
cargo run --bin fibc -- build --sysroot library --single-file samples/hello_world.fib
./samples/hello_world
```

An installed compiler with matching std sources does not need `--sysroot library`.
The compiler needs LLVM 21 and a host C linker. `run` remains unimplemented; invoke
the generated executable directly. For a project using a build file, see
[`example-project/`](../example-project/README.md).

## Validation

`tests/samples.rs` discovers every top-level `.fib` sample, preserves its filename,
builds it through the CLI in a temporary directory, then checks its exit code and
exact stdout/stderr. Each execution has a ten-second timeout. New samples require
an explicit expectation. Run the complete sweep with:

```sh
LLVM_SYS_211_PREFIX=/usr/lib/llvm/21 cargo test --offline --test samples
```

Parser-only discovery also works without LLVM:

```sh
cargo test --offline --no-default-features samples_parse_with_current_frontend
```

## Topics and exit codes

| Samples | Behavior |
| --- | --- |
| `hello_world`, `formatting`, `syscalls` | Byte output and typed I/O results; exit 0. |
| `minimal` | No output; exit 0. |
| `bitwise_ops`, `unsigned_ops` | IPv4 packing, flags, popcount and unsigned arithmetic; print decimal results; exit 0. |
| `enums`, `switch_enum`, `tagged_union` | Plain enums, exhaustive switches and direct payloads; print results; exit 0. |
| `sorting`, `slices` | Generic insertion sort and borrowed array mutation with checked indexing; print results; exit 0. |
| `linked_list`, `memory_pool` | Heap allocation and deferred cleanup; print results; exit 0. |
| `string_functions` | Byte-slice concatenation into caller storage, length and equality; prints `Hello, fib!`; exit 0. |
| `multiple_returns`, `nested_assign` | Struct return containing two named values, multi-binding locals and nested field assignment; print results; exit 0. |
| `function_pointer_callback` | Struct-stored callback; no output; exit 20. |
| `fib_bench` | Naive recursive Fibonacci through 20; prints results; exit 0. Increase its limit to 50 for a slow benchmark. |
| `builtin_scalars`, `scalar_loops`, `struct_storage`, `specification_runtime` | Scalars, flow, storage, generic arrays and readonly pointer access; no output; exit 42. |
| `enum_runtime`, `enum_payloads` | Constructors, payload copies, unit normalization and generic Result; no output; exit 42. |
| `const_bindings` | Immutable scalar/struct/enum bindings, readonly slice access and independent mutable copies; no output; exit 42. |
| `allocator`, `memory`, `generic_map` | Allocation/reuse, byte fill/copy and generic map lookup; no output; exit 42. |
| `indexed_globals`, `sizeof`, `deferred_blocks` | Global data and slices, target layout and reverse-order defer; no output; exit 42. |
| `process_entry` | Argument/environment startup and Result return; `FIB_EXIT=42` exits 42, otherwise an extra argument exits 0, and no extra argument exits 1. |

`specification_runtime` intentionally produces a compiler warning for implicit
pointer field access. `sizeof` checks the current Linux x86-64 layout.

Tuple returns are checked by analysis but native lowering remains unsupported;
`multiple_returns` uses an explicit struct to return both results. Slices borrow
their backing storage. `string_functions` returns a view into its caller's buffer,
and the heap examples free their allocations after use.

`deferred_blocks.fib` replaces `defer.fib`: source filenames become namespace
segments, which cannot be keywords. In `memory.fib`, the std alias is named `mem`
to avoid colliding with its own file namespace.
