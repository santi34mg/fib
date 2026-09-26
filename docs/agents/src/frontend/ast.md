# `src/frontend/ast/`

## Responsibility

The syntax AST preserves source-language structure before name and type
resolution. Major node groups are declarations, function signatures/bodies,
expressions, statements, type expressions, imports, fields, enum variants, and
switch patterns.

Expressions, statements, and type expressions carry a start `Span`. Many
declaration-level structures do not carry their own source range.

## Known Consequences

- signature diagnostics may fall back to the first body statement;
- empty function declarations can be spanless;
- import and type declaration analysis errors can be spanless;
- end positions collected by tokens are lost;
- typed AST and IR cannot retain source correlation they never receive.

## Improvement Direction

Give every AST node that can produce a diagnostic a `SourceId` and byte range,
including names, declarations, fields, variants, parameters, and patterns.
Keep syntax spelling and semantic identity separate. Avoid deriving diagnostic
locations from unrelated child statements.
