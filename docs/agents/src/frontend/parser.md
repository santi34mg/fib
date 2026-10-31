# Parser and current syntax

The [language specification](../../../language-specification.md) is implemented for prelude names, casts, shallow readonly views,
warned pointer field access and exhaustive switches. Concrete generic functions,
multiple source files and fixed-array values execute. Package-rooted lookup, scoped
visibility, installed std sources and Linux x86-64 syscalls execute. Recursive source
discovery remains deferred. Checked runtime indexing/slicing and constant-initialized
globals execute; value-returning `comp fn` execution remains scaffolding.

[`Parser`](../../../../crates/fib-frontend/src/frontend/parser.rs) consumes a token iterator and
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
- Enum variants use `Name` or `Name: Type`, including explicit struct payload
  types and eligible type-valued calls. Legacy `Name { field: Type }` is rejected
  with migration guidance. Analysis normalizes unit variants; their construction
  is a bare member value, with no parentheses.
- Index/slice syntax is `a[i]`, `a[start..end]`, with inclusive `.=` also parsed.
  Indexed compound assignments (`+=`, `-=`, `*=`, `/=`, `%=`) retain a single
  target, arithmetic operator and RHS in `IndexedCompoundAssignment`, including
  nested indices and calls. Analysis explicitly rejects this pending operation;
  simple-name compounds retain their existing desugaring. Other compound target
  forms remain rejected.
  Legacy `a.[i]`/`.@len` are not current syntax.
- Array/slice types use prefix `[N]T` and `[]T`; sentinel forms are `[N:S]T`,
  `[:S]T` and `[*:S]T`. Analysis currently requires uint8 sentinel elements.
- Pointer types use `*T`; `const *T`, `const []T` and `const [N]T` record
  layer-local element qualifiers independently of binding modifiers. Postfix
  `p*`/`x&` express dereference/address-of in supported delimiter contexts;
  projection assignment targets such as `p* = value` and `(p*).field = value`
  parse. Multiplication/bitwise-and remain binary operators.
- Loops use `while (condition) { ... };`, or `while () { ... };`, not `for`.
- Statements require `;`, including `if`, `while`, and `switch` blocks.
  Type bindings also require `;`, including after closing struct/enum braces.
  Top-level function definitions do not take an extra `;`.
  `defer` accepts a single statement or a brace-delimited block and owns one
  outer terminator; statements inside its block retain their terminators.
- Top-level ordinary statements/bindings are accepted by `parse()` alongside
  functions. `pub`, `const`, and `comp` on bindings record flags only, not semantic
  visibility or compile-time evaluation. Import has no supported loader or parsed
  import-declaration branch. Generic parameter lists are not parsed.

## Implementation map

Named type-returning calls such as `Map(K, V)` also parse in annotations, including
signature parameters/results and pointer/array/slice element types. The AST stores
`TypeExpressionKind::Computed` around the ordinary call expression; the shared
analyzer and bounded evaluator determine whether it denotes a type.

Per-concern modules include declaration/body/type parsing, postfix access/calls,
pointer, switch, tuple, and literal parsing. Low-to-high binary precedence:
`||`, `&&`, `|`, `^`, `&`, equality, comparison, `as`, shifts, additive,
multiplicative, unary, postfix/primary. `binary.rs` shares binary parsing.
Lookahead filters comments; former `@` builtins are rejected. `no_struct_literal`
resolves condition/body brace ambiguity.

No recovery pass exists. List/EOF handling and optional separators vary by parser;
do not infer a strict grammar from successful happy-path tests. Tests are in
`crates/fib-frontend/src/frontend/parser/test.rs`; add accepted and rejected cases with every syntax
change. No semantic claims follow from the returned AST.
