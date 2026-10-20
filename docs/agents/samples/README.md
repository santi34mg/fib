# Samples: parser regression coverage

[`samples/`](../../../samples) contains 15 `.fib` examples for output, arithmetic,
arrays/slices, enums, pointers, aggregates, and generic-related syntax. They use
bare bit-width types, direct indexing/slicing, `.len`, and current postfix pointer
syntax. Obsolete import declarations are removed; qualified `libc::` calls remain.

`frontend::parser::test::tests::samples_parse_with_current_frontend` automatically
discovers every top-level `.fib` file, parses it using the real tokenizer and
`Parser::with_source`, and rejects parse errors or empty ASTs. Run it with:

```sh
cargo test --no-default-features samples_parse_with_current_frontend
```

This is **parser-only coverage**, not type checking, compilation, or execution.
There is no e2e runner or working pipeline for these files. Output comments are
explicitly intended/unverified. CLI dispatch remains unfinished; do not recommend
historical positional-file or `-I` commands.

See the [sample guide](../../../samples/README.md) for dependency and semantic
limitations. In particular, a future build plan must supply a `libc` file, the
historical standard-library bindings are not migrated by this sample update,
and parsed enum member calls now receive payload-constructor type checks when
passed through the type resolver; the sample sweep itself only parses them. Preserve
example operations when migrating syntax; do not remove behavior just to parse.

For new syntax, retain positive/malformed in-memory parser tests as well as sample
discovery. When analysis/lowering works, add semantic and executable regression
coverage with explicit output/failure policies. No sample is currently certified
as an executable regression.
