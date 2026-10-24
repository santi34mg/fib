# Tokenizer — legacy lexer documentation URL

The old `src/frontend/lexer.rs` path is obsolete. Current implementation is
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
`int`/`uint` alias 32-bit types. `@` is a punctuation token interpreted with the
following name by the parser for builtin functions. Do not use legacy byte-width
mapping helpers as the current grammar; some comments and Display output are stale.

Unified declaration modifiers `pub`, `const`, and `comp` tokenize as keywords.
`type` remains the builtin metatype token for annotations and type values, not a
legacy `type Name ...` declaration introducer. The parser interprets `:=` as
inferred binding syntax and requires `;` on type-valued bindings too. Modifier
flags are preserved for analysis. The name pass enforces visibility and static
alias rules; arbitrary compile-time evaluation remains unfinished.

Tests live in `src/frontend/tokenizer/test.rs`. Add exact kinds, literal values,
positions, and malformed-input cases. Verify escape/backslash, NUL, Unicode and
numeric malformed-input behavior before making broader lexical guarantees.
Integer tokens retain arbitrary-precision magnitudes; floating tokens retain
decimal spellings. Contextual range checking and IEEE rounding run in analysis,
not tokenization. Escape decoding still occurs exactly once in the tokenizer.
