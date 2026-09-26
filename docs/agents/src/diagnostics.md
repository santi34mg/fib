# `src/diagnostics.rs`

## Responsibility

Diagnostics defines `ErrorKind`, point `Span`, `CompilerError`, source-line
extraction, error promotion, and rustc-like terminal rendering.

`CompilerError` currently carries a category, message, optional point span,
optional hint, filename, and one source line. The CLI is intended to be the only
printing path.

## Current Flow

- parser failures preserve filename, point position, and source line;
- analyzer failures begin as `AnalysisError` and are promoted by the driver;
- entry analysis errors may reread the source to add a panel;
- imported-module analysis errors keep a logical module name but no source line;
- backend, emit, and linker failures are mostly string-based by promotion time.

## Known Risks

- lexer failures become parser errors, leaving `ErrorKind::Lex` effectively unused;
- analyzer errors default to `Type`, including many missing-name failures;
- point spans discard token end positions and typed/IR nodes discard spans entirely;
- tabs and wide Unicode can misalign the caret;
- underlying error chains are often converted to strings;
- a source reread after analysis can display text different from what was compiled;
- several backend paths print errors directly and continue.

## Improvement Direction

Use `SourceId` plus byte ranges throughout AST, typed AST, and IR. Retain source
records for every module. Categorize errors at their origin and preserve causes.
Add range labels and machine-readable output only after the taxonomy and source
map are stable.
