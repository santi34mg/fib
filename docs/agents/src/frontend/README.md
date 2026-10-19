# Frontend

Active modules are [`tokenizer.rs`](../../../../src/frontend/tokenizer.rs),
`tokens/`, `parser.rs` plus `parser/`, `ast.rs` plus `ast/`, and
`analyzer.rs` plus `analyzer/`. There is no `lexer`, `analyze`, or `typed_ast`
source module in this checkout.

Working: source -> tokens -> syntax AST, including pending annotation slots.
Unified `[pub] [const | comp]` bindings include type-valued initializers
(`Name: type = ...;`); modifier flags are syntax, not implemented semantic policy.
Working utilities: lexical scope insertion/lookup/exit and separate labels.
Partial analysis: plan/unit ID validation, file declaration collection, and some
name traversal/lexical lookup. Name/alias/visibility resolution remains incomplete;
type/member resolution, generics, initialization and annotation validation are
sketches. The phased name-resolution suite specifies intended behavior with
ordinary failing tests, rather than certifying a completed analyzer. All analyzer
tests and helpers are in [`analyzer/test.rs`](../../../../src/frontend/analyzer/test.rs),
with six inline phases under `name_resolution`; see [validation](../../tests/README.md).

- [Tokenizer (legacy lexer page)](lexer.md)
- [Parser and current syntax](parser.md)
- [AST and annotations](ast.md)
- [Analyzer (legacy analyze page)](analyze/README.md)
- [Semantic types (legacy typed-AST page)](typed-ast.md)

Parsing a construct is not proof of semantic or runtime support. The parser
suite automatically discovers and parses all top-level `samples/*.fib` files;
these examples are syntax coverage, not semantic or executable regressions.
