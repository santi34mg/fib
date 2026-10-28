# Tokenizer

The [language specification](../../../language-specification.md) is implemented for prelude names, casts, shallow readonly views,
warned pointer field access and exhaustive switches. Concrete generic functions,
multiple source files and fixed-array values execute. Package-rooted lookup, scoped
visibility, installed std sources and Linux x86-64 syscalls execute. Recursive source
discovery remains deferred. Checked runtime indexing/slicing and constant-initialized
globals execute; value-returning `comp fn` execution remains scaffolding.

Current implementation is
[`tokenizer.rs`](../../../../src/frontend/tokenizer.rs) and its per-concern
`tokenizer/` modules; token definitions live in `tokens/`.

`Tokenizer::new(&str)` implements `Iterator<Item = Token>`. It traverses UTF-8
bytes with character-based line/column updates, starting at 1. Tokens retain
start/end positions. Comments remain tokens and parser lookahead skips them.
Malformed text produces `Error(String)` or `Unknown(char)` tokens, not a lexical
Result. Parser paths surface those as parse failures.

Implemented tokenization includes Unicode identifiers, line comments, numeric
bases, decimal floats, string/character escapes, operators and punctuation.
Bare canonical builtin type names use bit widths (`int8`, `uint64`, `float32`);
`int`/`uint` alias 32-bit types. Primitive spellings emit ordinary identifier tokens and resolve through an
implicit prelude; protected bare type names cannot be shadowed. `@` remains
punctuation for diagnostics; former builtin-call spellings are rejected.

Unified declaration modifiers `pub`, `const`, and `comp` tokenize as keywords.
`type` is an ordinary identifier denoting the prelude metatype; legacy
`type Name ...` declarations do not parse. The parser interprets `:=` as
inferred binding syntax and requires `;` on type-valued bindings too. Modifier
flags are preserved for analysis. The name pass enforces visibility and static
alias rules; arbitrary compile-time evaluation remains unfinished.

Tests live in `src/frontend/tokenizer/test.rs`. Add exact kinds, literal values,
positions, and malformed-input cases. Verify escape/backslash, NUL, Unicode and
numeric malformed-input behavior before making broader lexical guarantees.
Integer tokens retain arbitrary-precision magnitudes; floating tokens retain
decimal spellings. Contextual range checking and IEEE rounding run in analysis,
not tokenization. Escape decoding still occurs exactly once in the tokenizer.
