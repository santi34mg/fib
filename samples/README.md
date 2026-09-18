# Samples

Working Fib programs exercised end-to-end by `tests/e2e.rs`: each sample is
compiled through the library API and executed, with exact-stdout asserts for
the deterministic cases (`fib_bench` is compile-only). The expected output is
recorded in an `Expected output:` header comment inside each file — the same
strings the e2e harness asserts on.

Run any sample from the project root:

```bash
cargo run -- samples/hello_world.fib -I=std
./out/hello_world
```

`-I` adds an import search directory (`std/` here, so `import std::libc`
resolves). The entry file's own directory is always searched first.

## Files

- `hello_world.fib` — minimal `libc::printf` program.
- `minimal.fib` — smallest valid program (`fn main`, no imports, no output).
- `bitwise_ops.fib` — shifts/masks over `@uint4` (IPv4 packing, permission flags, popcount).
- `enums.fib` — plain enum discriminant cast (`Color.Green` → `1`).
- `switch_enum.fib` — `switch`/`when` dispatch over a plain enum.
- `tagged_union.fib` — enum variants with payloads (`Integer`/`Boolean`/`EOF`).
- `sorting.fib` — generic `insertion_sort(T: type, ...)` over `@int4[8]`.
- `string_builtins.fib` — `@concat`/`@str_len`/`@str_eq` over `@string`.
- `multiple_returns.fib` — multi-value return (`divmod` → `q, r`).
- `nested_assign.fib` — nested struct field assignment (`o.inner.value = 42`).
- `linked_list.fib` — heap list via `malloc`/`free` with `defer` cleanup.
- `memory_pool.fib` — fixed-block bump allocator with exhaustion handling.
- `fib_bench.fib` — naive recursive `fib(50)` benchmark (compile-only in e2e; takes hours to run).
