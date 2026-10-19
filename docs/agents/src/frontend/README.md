# Frontend

Active modules are [`tokenizer.rs`](../../../../src/frontend/tokenizer.rs),
`tokens/`, `parser.rs` plus `parser/`, `ast.rs` plus `ast/`, and
`analyzer.rs` plus `analyzer/`. There is no `lexer`, `analyze`, or `typed_ast`
source module in this checkout.

Working: source -> tokens -> syntax AST, including pending annotation slots.
Unified `[pub] [const | comp]` bindings include type-valued initializers.
Analysis requires explicit `comp` for type bindings (`comp Name: type = ...;`).
Working name resolution: plan/unit validation, annotated declaration collection,
lexical lookup/order, qualified paths, visibility, and compile-time aliases with
cycle detection. Lexical scope utilities also provide separate label storage.
Type/member resolution, generics, initialization and full completion validation
remain sketches. All analyzer tests/helpers are in
[`analyzer/test.rs`](../../../../src/frontend/analyzer/test.rs), including six
passing inline phases under `name_resolution`; see [validation](../../tests/README.md).

- [Tokenizer (legacy lexer page)](lexer.md)
- [Parser and current syntax](parser.md)
- [AST and annotations](ast.md)
- [Analyzer (legacy analyze page)](analyze/README.md)
- [Semantic types (legacy typed-AST page)](typed-ast.md)

Parsing a construct is not proof of semantic or runtime support. The parser
suite automatically discovers and parses all top-level `samples/*.fib` files;
these examples are syntax coverage, not semantic or executable regressions.
