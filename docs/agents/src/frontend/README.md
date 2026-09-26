# `src/frontend/`

## Pipeline

```text
source text
  -> lexer.rs + tokens/
  -> parser.rs + parser/*
  -> ast/*
  -> analyze.rs + analyze/*
  -> typed_ast.rs
```

The parser produces a syntax tree with source-oriented shapes. Analysis resolves
names and modules, maps types, checks semantics, monomorphizes generic calls,
and produces the typed representation consumed by both lowering routes.

## Mirrored Pages

- [`lexer.md`](lexer.md): tokenization, literals, comments, and positions
- [`parser.md`](parser.md): parser state, precedence, statements, and EOF behavior
- [`ast.md`](ast.md): syntax-tree ownership and span coverage
- [`analyze/`](analyze/README.md): semantic passes and missing invariants
- [`typed-ast.md`](typed-ast.md): symbols, types, declarations, and lowering contract

## Intended Boundary

If analysis succeeds, every typed declaration should be name-resolved,
well-typed, finite-layout, control-flow valid, and lowerable by at least one
backend route. Today that boundary is incomplete. See the analyzer page and P0
roadmap before extending backend assumptions.

## Feature Snapshot

Implemented language areas include scalar builtins, explicit/inferred locals,
functions and externs, structs, enums with payloads, raw pointers, arrays,
slices, tuples/multiple returns, explicit type-parameter generics, recursive
imports, C-style loops, `defer`, and enum switch.

Partially implemented or reserved areas include function values, constants,
`union`, complete `@never` behavior, module visibility, target-aware `@usize`,
and qualified nominal types through all analysis/lowering paths.
