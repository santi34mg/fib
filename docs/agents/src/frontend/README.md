# Frontend

The [language specification](../../../language-specification.md) is implemented for prelude names, casts, shallow readonly views,
warned pointer field access and exhaustive switches. Concrete generic functions,
multiple source files and fixed-array values execute. Package metadata/scoped
visibility exist; package-rooted lookup/discovery and runtime indexing/slices remain deferred.

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
Semantic types, expressions and owner-based members are resolved and validated on
the original AST; see [type resolution](../../../type-resolution.md). Explicit metatype
specialization, bounded type-return evaluation, definite initialization and
structured flow checks run through `Analyzer::analyze`. All analyzer tests/helpers are in
[`analyzer/test.rs`](../../../../src/frontend/analyzer/test.rs), including six
passing inline phases under `name_resolution`; see [validation](../../tests/README.md).

- [Tokenizer](tokenizer.md)
- [Parser and current syntax](parser.md)
- [AST and annotations](ast.md)
- [Analyzer](analyzer.md)
- [Semantic types](semantic-types.md)

Parsing a construct is not proof of semantic or runtime support. The parser
suite automatically discovers and parses all top-level `samples/*.fib` files;
these examples are syntax coverage, not semantic or executable regressions.
