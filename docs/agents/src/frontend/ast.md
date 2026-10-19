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

Unified bindings use `VariableDeclaration` with independent `is_pub`, `is_const`,
and `is_comp` flags; `const` and `comp` are mutually exclusive in the grammar.
An explicit annotation is stored in `ty`, and the optional initializer in
`expression`. Typed declarations may omit initializers for compatibility.
`comp Name: type = ...;` is a variable binding with an arbitrary expression
initializer, not a parser-produced `TypeDeclaration`. Explicit type syntax such
as `struct { ... }` produces `ExpressionKind::TypeValue(TypeExpression)`;
named references such as `pkg::Value` and type-producing calls remain ordinary
`QualifiedIdentifier` and `Call` expressions. Semantic analysis must determine
whether these expressions denote types. Initializers retain pending expression
annotations, enclosed type syntax retains pending type annotations, and the
binding is annotated by declaration collection/name resolution. Analysis rejects
type bindings without explicit `comp`; the parser retains that syntax.
Multi-bindings retain individual declaration records and a separate shared
initializer list introduced by the final `:=` (or typed `=`).

Name/binding completion is validated by the name pass. Full semantic completion
validation is unimplemented. A public AST or `AnalyzedProject` with
filled-looking records is not evidence that semantic checking ran.
