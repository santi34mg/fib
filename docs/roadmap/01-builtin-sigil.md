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
String operations will be ordinary standard-library functions. No replacement
library signatures, allocation contracts or runtime implementations are provided
by this compiler change. Compiler intrinsics still resolve by declaration identity
in synthetic namespaces. `@` is tokenized for diagnostics, not a working
attribute system.

`.len` remains ordinary member syntax. Semantic lookup distinguishes a struct's
field from the array/slice/string length member and returns symbolic `usize` for
the latter. See [implemented typing](../type-resolution.md).

## Library follow-up

Define string APIs, allocation/failure/ownership contracts and executable library
coverage alongside [the string redesign](05-remove-string.md). The historical
std wrapper around `@concat` has been retired. Samples of ordinary string helper
calls contain declarations only and do not claim working implementations.

Tokenizer/parser, type-resolution and editor-highlighting tests cover the
compiler-side removal. Library equivalence/runtime tests require working standard-library implementations and string lowering; the
scalar/aggregate execution pipeline and executable harness already exist.
