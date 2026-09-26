# `src/frontend/typed_ast.rs`

## Core Types

- `TypedProgram`: symbol table, local declarations, imported declarations.
- `TypedModule`: exports and declarations for an analyzed module.
- `SymbolTable`: lexical frames plus module aliases.
- `Ty`: builtins, nominal identifiers, structs, enums, pointers, arrays,
  slices, functions, tuples, qualified identifiers, and metatype.
- `TypedDecl`: functions, types, and a currently unparsed constant form.
- `TypedExpr` / `TypedStatement`: semantic operators and checked statement forms.
- `TypedBinding` / `TypedFunction`: lowered declaration data.

Semantic `BinOp` and `LogicalOp` are separate from token operators, which is a
useful boundary: reserved or assignment-only syntax cannot accidentally enter
normal binary lowering.

## Current Invariants

- frame zero is global;
- lookup is lexical from inner to outer frames;
- imported module aliases live outside lexical frames;
- plain source names remain the primary identity for many bindings/functions;
- typed nodes generally do not retain source spans.

`ScopeKind` is accepted by scope entry but currently ignored. Termination helper
logic exists but is not a reliable all-path control-flow proof.

## Improvement Direction

Introduce stable IDs for modules, symbols, bindings, and nominal types. Typed
identifier/lvalue/call nodes should refer to IDs rather than reconstructing
identity from source text. Preserve source ranges on typed nodes for downstream
diagnostics. Keep module ownership with declarations instead of recursively
copying and deduplicating them by name.
