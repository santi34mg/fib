# 01 — Primitive spelling and removal of builtin functions

**Implemented:** primitive type names are ordinary identifiers
resolved to compiler-owned type values through an implicit prelude; those bare
names cannot be shadowed. The tokenizer emits ordinary identifier tokens; analysis supplies the implicit prelude.
See [language specification](../language-specification.md#names-packages-and-source-discovery).

## Implemented baseline

Types use bare bit-width names (`int32`, `uint64`, `float32`); `int`/`uint` are
32-bit aliases. Legacy `@` types and byte-width `int4` are rejected.

Builtin functions have been removed from tokens, syntax and analysis. `concat`,
`str_len` and `str_eq` are ordinary identifiers; their former `@` spellings fail.
String operations are ordinary standard-library functions with byte-slice
implementations described below. Compiler intrinsics still resolve by declaration identity
in synthetic namespaces. `@` is tokenized for diagnostics, not a working
attribute system.

`.len` remains ordinary member syntax. Semantic lookup distinguishes a struct's
field from the array/slice length member and returns symbolic `usize` for
the latter. See [implemented typing](../type-resolution.md).

## Completed library follow-up

`std::string::concat(destination: []uint8, a: const []uint8, b: const []uint8)`
returns `std::string::ConcatResult`, a `Result([]uint8, std::string::Error)`.
It checks both input lengths against capacity without overflow before writing.
Insufficient capacity returns `BufferTooSmall` with destination unchanged.
Success returns a view of the written prefix, preserves the unused tail and
appends no NUL. It allocates nothing; sources must not overlap destination.
The result borrows caller-owned storage without extending its lifetime.

`std::string::str_len(value: const []uint8) usize` wraps `.len`, counting bytes.
The same module supplies ordinary `std::string::str_eq(const []uint8, const []uint8) bool`
with bytewise content equality, including empty slices. No encoding is assumed.
The historical std wrapper around `@concat` has been retired; the string sample
uses these working source-library implementations.

Tokenizer/parser, type-resolution and editor-highlighting tests cover the
compiler-side removal. `tests/programs/string_helpers.fib` and
`tests/standard_library.rs` cover lowering and native execution: content equality,
byte length, exact/insufficient capacity, unchanged failure storage, empty inputs,
preserved tails and borrowed result mutation. The `string_functions` sample uses
the same APIs. Owning buffers, encoding policy and C interoperability remain
separate work under [the string redesign](05-remove-string.md); they do not block
this roadmap item.
