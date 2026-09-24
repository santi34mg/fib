# `src/frontend/lexer.rs` and `src/frontend/tokens/`

## Representation

`Lexer` is an iterator over `Token`. A token contains its `TokenKind` plus start
and end positions. Positions are byte traversal state expressed as 1-based line
and character columns.

Malformed input is represented in the token stream as `Unknown(char)` or
`Error(String)` rather than as `Result<Token, LexError>`. The parser later turns
these tokens into parse errors.

`tokens/` centralizes:

- keywords;
- operators;
- punctuation;
- builtin types, functions, and properties;
- literal values;
- token/span structures.

Comments remain tokens and are skipped by parser lookahead.

## Current Lexical Features

- Unicode alphabetic/alphanumeric identifiers;
- line comments;
- decimal plus explicit decimal, binary, octal, and hexadecimal integers;
- decimal floating literals;
- strings, characters, booleans, and `null`;
- `@`-prefixed builtins;
- multi-character and compound operators.

## Known Risks

- strings are escape-decoded in the lexer and decoded again during analysis;
- string and character escapes use different validation policy;
- integer literal storage is `u64` while 128-bit integer types exist;
- contextual integer range checks are absent;
- exponent notation and separators are unsupported;
- reserved `__` identifiers are documented but not rejected;
- lexical failures are categorized as parse failures.

## Test Guidance

Lexer tests live in `src/frontend/lexer/test.rs`. Add exact token/value/range
tests for every literal change and malformed-input no-panic tests. Escape
handling changes must explicitly cover a literal backslash followed by `n`, a
trailing escaped backslash, Unicode escapes, and embedded NUL.
