# Semantic types — obsolete typed-AST path

`src/frontend/typed_ast.rs`, `TypedProgram`, `TypedModule` and `Ty` are gone.
This URL is retained to redirect old implementation guidance to the current
[annotated AST](ast.md) and [analyzer](analyze/README.md).

[`semantic_type.rs`](../../../../src/frontend/semantic_type.rs) defines project-local
`TypeId` and `SemanticType` records: builtins, pointers, arrays, slices, tuples,
functions, structs, enums, declaration-backed types and binding-identified generic
parameters. `SemanticTypes` holds type records plus declaration/local type maps.
It is a raw workspace: interning, normalization, inference, recursive-type legality
and layout are not implemented.

Expression annotation information includes semantic type, value/place category,
and optional denoted type for type-valued expressions. Names, bindings and member
identities are separate. `AnalyzedProject` owns the same source trees and semantic
tables, not copied typed declarations. Planned lowering must translate these into
IR-owned runtime types/IDs, not borrow frontend semantic tables or fall back to AST
emission. The name pass validates name/binding completeness; full semantic
annotation validation remains unfinished.
