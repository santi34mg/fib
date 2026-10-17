# Literals, identifiers, and comments

> **Verified parser behavior:** The literal, identifier, comment, and whitespace forms below are recognized.

## Comments and whitespace

`//` starts a comment through the end of the line. The parser filters comment
tokens. There is no block-comment syntax (`/* ... */`). Whitespace, including
newlines, does not terminate a statement; `;` is required.

```fib
// A comment before a declaration.
count: int32 = 42; // A trailing comment.
```

## Integer literals

| Prefix | Base | Examples |
|--------|------|----------|
| None or `0d` | Decimal | `42`, `0d42`, `0123` |
| `0x` | Hexadecimal | `0xFF` |
| `0o` | Octal | `0o17` |
| `0b` | Binary | `0b1010` |

Prefixes are lowercase. A leading zero alone does not imply octal. Token payloads
are parsed as `u64`; values beyond that range are tokenizer errors, even though
128-bit type names exist. A negative number is a unary minus expression applied
to an unsigned integer-literal token, not a signed token.

Underscore separators, numeric suffixes, and uppercase base prefixes are not
implemented as parts of numeric literals. Invalid trailing characters may become
separate tokens rather than one numeric diagnostic.

## Floating-point literals

```fib
value: float64 = 3.25;
negative := -0.5;
```

A decimal fractional point is consumed only if followed by a decimal digit.
Literal payloads are parsed as Rust `f64`; the annotation does not change that
lexical representation. Decimal exponent syntax, hexadecimal floats, `.5`, and
`1.` are not supported as single float literals. `1..5` is tokenized as an
integer, the `..` range separator, and another integer; it is not a standalone
range expression in the parser. See [Arrays](arrays.md).

## Booleans and null

```fib
yes := true;
no := false;
ptr: *int32 = null;
```

`true`, `false`, and `null` are literal tokens. See
[Pointers and memory](pointers-memory.md) for pointer syntax.

## Characters

Single quotes enclose one character. Supported escapes are `\n`, `\t`, `\0`,
`\\`, `\'`, `\"`, `\xNN` (exactly two hex digits), and `\u{...}` (a valid
Unicode scalar value in hex). `\r` is not a supported character escape.

```fib
letter := 'a';
newline := '\n';
hex := '\x41';
unicode := '\u{41}';
```

The token holds a Rust `char`. Unknown escapes, invalid scalar values, missing
closing quotes, and multiple-character literals are rejected.

## Strings

Double quotes enclose string contents. Recognized escapes are `\n`, `\t`, `\r`,
`\0`, `\\`, `\"`, and `\'`. Unlike characters, unknown string escapes are
preserved as a backslash plus the following character; `\xNN` and `\u{...}`
are not decoded in strings. Unterminated strings/escapes produce error tokens.
The scanner also permits literal newlines inside strings.

```fib
message := "line one\nline two\n";
```

## Identifiers

An identifier starts with an alphabetic character or `_`, followed by alphanumeric
characters or `_` (using Rust character predicates, not an ASCII-only rule).
Keywords and builtin names are classified before ordinary identifiers.

Double-underscore names are described in a tokenizer comment as intended for
compiler internals, but the tokenizer does **not** reject them. Do not describe
this as an enforced reservation. `var`, `for`, `import`, `comp`, and `pub` are
currently ordinary identifiers, not active keywords; that does not give them
the old or planned declaration semantics.

See [`src/frontend/tokenizer/test.rs`](../../src/frontend/tokenizer/test.rs) for lexical tests and
[`src/frontend/parser/test.rs`](../../src/frontend/parser/test.rs) for literal-expression and lexer-error propagation tests.
