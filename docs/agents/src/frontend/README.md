# Frontend

The [language specification](../../../language-specification.md) is implemented for prelude names, casts, shallow readonly views,
warned pointer field access and exhaustive switches. Concrete generic functions,
multiple source files and fixed-array values execute. Package-rooted lookup, scoped
visibility, installed std sources and Linux x86-64 syscalls execute. Recursive source
discovery remains deferred. Checked runtime indexing/slicing and constant-initialized
globals execute; value-returning `comp fn` execution remains scaffolding.

Active modules are [`tokenizer.rs`](../../../../crates/fib-frontend/src/frontend/tokenizer.rs),
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
[`analyzer/test.rs`](../../../../crates/fib-frontend/src/frontend/analyzer/test.rs), including six
passing inline phases under `name_resolution`; see [validation](../../tests/README.md).

- [Tokenizer](tokenizer.md)
- [Parser and current syntax](parser.md)
- [AST and annotations](ast.md)
- [Analyzer](analyzer.md)
- [Semantic types](semantic-types.md)

Parsing a construct is not proof of semantic or runtime support. The parser
suite automatically discovers and parses all top-level `samples/*.fib` files;
these examples are syntax coverage, not semantic or executable regressions.

## Editor analysis

The native `fib-lsp` uses `frontend::tooling` for compiler-backed tokens and
collecting diagnostics. `source::SourceProvider` shares disk/overlay loading
between parsing and build planning. Open documents override source reads;
compilation continues to use disk sources by default.

`Analyzer::analyze_collecting` and `analyze_available` return `AnalysisReport`,
which deliberately cannot enter lowering. Recovery tracks failed statements and
bindings, skips dependent work, and keeps successful annotations for highlighting.
Unavailable source files retain explicit blocked identities. Parser failures
remain first-error-per-file; available files continue analysis. Generic instance
rechecking stays strict within each instance, with failures collected by callers.
`Analyzer::analyze` and annotation validators retain their strict contracts.
