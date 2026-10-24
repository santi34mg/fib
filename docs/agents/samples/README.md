# Samples: parser regression coverage

[`example-project/`](../../../example-project/README.md) is the runnable current-
capabilities project. Its two source files cover scalars/control flow plus
structs/pointers, enums/switches, generics, fixed arrays and multi-binding locals.
Driver tests verify both planned files, the entry/output and source IR without
LLVM; executable acceptance builds it through the CLI and checks exit 0.

[`samples/`](../../../samples) contains `.fib` examples for output, arithmetic,
arrays/slices, enums, pointers, aggregates, and generic-related syntax. They use
bare bit-width types, direct indexing/slicing, `.len`, and current postfix pointer
syntax. Obsolete import declarations are removed; qualified `libc::` calls remain.

`frontend::parser::test::tests::samples_parse_with_current_frontend` automatically
discovers every top-level `.fib` file, parses it using the real tokenizer and
`Parser::with_source`, and rejects parse errors or empty ASTs. Run it with:

```sh
cargo test --no-default-features samples_parse_with_current_frontend
```

The discovery sweep is parser-only coverage. `builtin_scalars.fib` and
`scalar_loops.fib`, `struct_storage.fib` and `enum_runtime.fib` additionally have LLVM compile/link/run coverage in `tests/executable.rs`, returning 42. Build
them with `fibc build --single-file PATH`. Other samples'
output comments remain intended/unverified. Historical positional-file and `-I`
commands remain unsupported.

See the [sample guide](../../../samples/README.md) for dependency and semantic
limitations. In particular, a future build plan must supply a `libc` file, the
historical standard-library bindings have been removed,
and parsed enum member calls now receive payload-constructor type checks when
passed through the type resolver; the sample sweep itself only parses them. Preserve
example operations when migrating syntax; do not remove behavior just to parse.

For new syntax, retain positive/malformed in-memory parser tests as well as sample
discovery. When analysis/lowering works, add semantic and executable regression
coverage with explicit output/failure policies. The scalar and loop samples are covered as executable regressions.

`specification_runtime.fib` has executable acceptance coverage and exits 42. It
combines ordinary prelude types, generic arrays, bool casts, warned pointer field
access and exhaustive switches. Its warning is intentional.
