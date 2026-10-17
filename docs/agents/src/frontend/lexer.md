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

Tests live in `src/frontend/tokenizer/test.rs`. Add exact kinds, literal values,
positions, and malformed-input cases. Verify escape/backslash, NUL, Unicode and
numeric overflow behavior before making broader lexical guarantees. No analyzer
executes to perform a second escape-decoding or contextual range-check pass.
