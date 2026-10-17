# AST and annotation ownership

[`ast.rs`](../../../../src/frontend/ast.rs) defines `Ast { statements }`.
`ast/` splits expressions, statements, functions/signatures/parameters, types,
patterns, fields, enum variants, qualified names and switch syntax. Actual
spellings include `structl_field.rs`, `ExprAcess`, and `BinaryExpresison`; do not
invent corrected source paths in tooling.

Analysis enriches this original tree in place; it does not manufacture a second
typed AST. [`analysis_annotation.rs`](../../../../src/frontend/analysis_annotation.rs)
defines pending name, binding, type, expression and member slots. Parsing leaves
all applicable resolution slots `None`; pending never means successfully unknown.
Names/qualified paths retain spelling, and expressions/statements/type expressions
retain start spans. See [diagnostics/source locations](../diagnostics.md).

Bindings use declaration or local identities; members use owner/type plus field,
variant or length identity, not lexical bindings. Function bindings belong on
signatures; parameter and multi-binding sites have their own slots. Label slots
are reserved but not embedded because label syntax is undefined. The generic
parameter string list is reserved and not parsed. Legacy `QualifiedAccess` is not
the parser-produced annotated qualified-name representation.

Completion validation is unimplemented. A public AST or `AnalyzedProject` with
filled-looking records is not evidence that semantic checking ran.
