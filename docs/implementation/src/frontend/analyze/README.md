# `src/frontend/analyze.rs` and `src/frontend/analyze/`

## Pass Structure

`analyze` currently:

1. processes imports and inserts module aliases or selected exports;
2. copies imported declarations for later whole-program lowering;
3. walks local declarations in source order;
4. resolves each declaration and immediately analyzes its body;
5. appends cached generic instantiations sorted by mangled name;
6. returns `TypedProgram` with a symbol table, local declarations, and imported
   declarations.

Submodules own functions, types, expressions, statements, and generic
substitution.

## Name and Scope Model

`SymbolTable` has a stack of lexical frames plus a separate module-alias map.
Lookup searches inner to outer frames. Module aliases are global to the table.
Insertion returns a replaced symbol, but most callers ignore it, so duplicate
names commonly overwrite earlier names without a diagnostic.

Functions are not all predeclared in a first pass. Recursion works because a
function signature is inserted before its own body; calls to later functions
generally require a forward declaration.

## Coercion and Type Checking

Analysis inserts typed expression forms and performs many useful checks:
ordinary assignments, calls, struct construction, switch exhaustiveness,
loop-control placement, slices, and constant bounds. Numeric coercion is
largely contextual and asymmetric: binary RHS values are often coerced to the
LHS type.

## Missing Frontend Invariants

The following can currently survive analysis or are incompletely checked:

- non-boolean if/loop conditions;
- float bitwise/shift operations and broad pointer binary operations;
- unsupported casts;
- `null` coerced to non-pointer targets through `@never`;
- reads of uninitialized locals;
- non-void reachable fallthrough;
- type mismatches in multiple assignment;
- bare construction of payload enum variants;
- duplicate declarations, fields, variants, aliases, and locals;
- unknown or qualified types in some contexts;
- cyclic aliases and recursive-by-value layout;
- generic runtime arity and substitution through all AST variants;
- root mutability through nested lvalues.

These are correctness issues because lowering assumes stronger types and
control flow than analysis proves.

## Generic Model

Functions with `type` parameters are retained as templates and instantiated at
explicit generic calls. Instantiations are cached by a string mangle and added
to declarations in sorted order. Current mangles collapse broad anonymous type
classes and substitution does not visit every expression/type position.

Prefer a canonical structural type fingerprint and one exhaustive visitor over
variant-by-variant partial substitution.

## Diagnostic Model

`AnalysisError` defaults to `ErrorKind::Type`; `with_kind` exists but is rarely
used. Spans are attached while bubbling through syntax nodes. Internal helper
names appear in several user-facing messages.

Use `Name` for resolution/duplicate errors, preserve precise ranges, and render
types in source spelling rather than `Debug` output.

## Test Guidance

Analyzer tests are in `src/frontend/analyze/test.rs` and use in-memory sources.
For every soundness fix, add `is_err()` plus a stable message/category/range
substring. Do not require full diagnostic string equality. Add success tests
when a nearby legal construct could regress.
