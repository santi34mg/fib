# Parser and current syntax

[`Parser`](../../../../src/frontend/parser.rs) consumes a token iterator and
returns `anyhow::Result<Ast>` from `parse()`. For in-memory tests:

```rust
use std::path::Path;
use fibc::frontend::{parser::Parser, tokenizer::Tokenizer};
let source = "fn main() int32 { value := 1; return value; }";
let ast = Parser::with_source(Tokenizer::new(source), Path::new("test.fib"), source)
    .parse().unwrap();
assert_eq!(ast.statements.len(), 1);
```

This is parsing only. It does not type-check or compile the function.

## Grammar currently exercised

- Bare builtin types: `int32`, `uint64`, `bool`, `string`, `usize`, `type`;
  `@int32` is rejected. Functions are ordinary identifiers; former builtin `@` calls are rejected.
- Unified bindings: `[pub] [const | comp] name (: Type = | :=) expression;`.
  `pub` precedes the optional, mutually exclusive `const`/`comp`. No `var` prefix.
  For compatibility, typed declarations without initializers (`x: int32;`) remain
  supported. Each multi-binding has independent modifiers and an optional type;
  uninitialized multi-bindings require a type on each binding. A final `:=`
  introduces the shared initializer list, not a shared type.
- Functions: `fn add(a: int32, b: int32) int32 { return a + b; }`;
  omitted return type is void. Prototypes/externs use `;`. Explicit `comp fn`
  declarations and `comp fn(...) -> type` function types parse separately.
  Variadic parameter syntax is removed.
- Type bindings use `Name: type = struct { field: int32 };` or
  `Alias: type = pkg::Value;`, producing `VariableDeclaration` with an arbitrary
  expression initializer. Explicit type syntax produces `TypeValue`; named
  references and calls remain ordinary expressions, with type/value discrimination
  left to semantic analysis. Legacy `type Name ...` declaration parsing is removed.
- Qualified names use `file::name`; members use `value.field`; length is `.len`.
- Index/slice syntax is `a[i]`, `a[start..end]`, with inclusive `.=` also parsed.
  Legacy `a.[i]`/`.@len` are not current syntax.
- Pointer types use `*T`; `const *T`, `const T[]` and `const T[N]` record
  layer-local element qualifiers independently of binding modifiers. Postfix
  `p*`/`x&` express dereference/address-of in supported delimiter contexts;
  projection assignment targets such as `p* = value` and `(p*).field = value`
  parse. Multiplication/bitwise-and remain binary operators.
- Loops use `while (condition) { ... };`, or `while () { ... };`, not `for`.
- Statements require `;`, including `if`, `while`, and `switch` blocks.
  Type bindings also require `;`, including after closing struct/enum braces.
  Top-level function definitions do not take an extra `;`.
  `defer` owns one terminator for its inner statement.
- Top-level ordinary statements/bindings are accepted by `parse()` alongside
  functions. `pub`, `const`, and `comp` on bindings record flags only, not semantic
  visibility or compile-time evaluation. Import has no supported loader or parsed
  import-declaration branch. Generic parameter lists are not parsed.

## Implementation map

Per-concern modules include declaration/body/type parsing, postfix access/calls,
pointer, switch, tuple, and literal parsing. Low-to-high binary precedence:
`||`, `&&`, `|`, `^`, `&`, equality, comparison, `as`, shifts, additive,
multiplicative, unary, postfix/primary. `binary.rs` shares binary parsing.
Lookahead filters comments and normalizes `@` builtins; `no_struct_literal`
resolves condition/body brace ambiguity.

No recovery pass exists. List/EOF handling and optional separators vary by parser;
do not infer a strict grammar from successful happy-path tests. Tests are in
`src/frontend/parser/test.rs`; add accepted and rejected cases with every syntax
change. No semantic claims follow from the returned AST.
