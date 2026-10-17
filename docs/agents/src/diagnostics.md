# Diagnostics and source locations

[`diagnostics.rs`](../../../src/diagnostics.rs) implements `ErrorKind`,
`Span`, `CompilerError`, rendering, and `source_line_at`. `Span` is a 1-based
start line/column, not a byte range. `CompilerError` holds optional filename,
source line, span, and hint. Parser-error conversion preserves source context;
analyzer conversion is commented out. No complete driver-wide diagnostic funnel
exists: sketches use anyhow errors, and CLI dispatch still panics.

## Current source map limits

There is no centralized source-map database. `BuildPlan` associates file identity,
logical name, and disk path; `SourceUnit` associates that identity with an AST.
Tokens retain start/end positions, while AST expressions/statements/type expressions
retain point spans, not full ranges. `Parser::with_source` snapshots source lines
for errors. `Parser::new` reads the filename and silently uses empty text on read
failure. Prefer `with_source` in tests to avoid disk and source mismatches.

`source_line_at` extracts a 1-based line. Rendering uses spaces for caret alignment;
tabs and wide Unicode can misalign it. IR has no comprehensive source-map plumbing.
Future passes should retain source records and byte ranges keyed by file identity,
not reread sources or infer locations from unrelated nodes.

Tests cover parser promotion, panels/hints, plain rendering, line extraction and
Display. Assert category, stable substring and relevant location rather than full
rendered diagnostic equality when adding new failure cases.
