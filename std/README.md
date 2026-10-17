# Fib standard-library sketches

`std/` contains historical C bindings and early standard-library source sketches.
**No module is currently usable through an end-to-end compiler pipeline.**
Analysis, module loading, build orchestration, code generation, and linking are
unfinished. The current parser does not implement `import` declarations, and
the CLI no longer accepts `-I` search paths.

The source files may contain obsolete syntax. In particular, historical
`@string` is now bare `string`, and byte-width `@int4` is now bit-width `int32`.
Current `int` is a 32-bit alias. Builtin functions still use `@`. See the
[current syntax reference](../docs/README.md) and
[planned module rules](../docs/parsing/imports.md).

## Source inventory and intent

- `libc.fib`: historical `extern` C bindings such as `printf`, `malloc`, `free`,
  `memcpy`, `exit`, `strlen`, and `puts`. Parsing `extern` declarations does not
  establish working ABI classification, linking, or memory safety.
- `core/error.fib`: proposed error record.
- `fs/path.fib`: proposed path wrapper and conversion helpers.
- `io/fd.fib`: file-descriptor placeholder.
- `io/stream.fib`: stream API scaffolding using historical imports.

The future standard-library design is tracked in the [roadmap](../docs/roadmap/README.md).
Neither source presence nor expected signatures imply tested runtime support.
