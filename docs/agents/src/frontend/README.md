# Frontend

Active modules are [`tokenizer.rs`](../../../../src/frontend/tokenizer.rs),
`tokens/`, `parser.rs` plus `parser/`, `ast.rs` plus `ast/`, and
`analyzer.rs` plus `analyzer/`. There is no `lexer`, `analyze`, or `typed_ast`
source module in this checkout.

Working: source -> tokens -> syntax AST, including pending annotation slots.
Working utilities: lexical scope insertion/lookup/exit and separate labels.
Sketched: project declarations, name/alias/visibility resolution, type/member
resolution, generics, initialization and annotation validation.

- [Tokenizer (legacy lexer page)](lexer.md)
- [Parser and current syntax](parser.md)
- [AST and annotations](ast.md)
- [Analyzer (legacy analyze page)](analyze/README.md)
- [Semantic types (legacy typed-AST page)](typed-ast.md)

Parsing a construct is not proof of semantic or runtime support. The parser
suite automatically discovers and parses all top-level `samples/*.fib` files;
these examples are syntax coverage, not semantic or executable regressions.
