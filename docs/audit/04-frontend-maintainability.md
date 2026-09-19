# Frontend Maintainability: Lexer, Parser, AST, Analyze

> Note (2026-09-18): addressed items removed. This file has NO remaining
> points. Removed: generic `parse_binary` with thin wrappers
> (`parser/binary.rs`), `atom.rs` split (`primary/access/call/struct_literal`)
> + `lex_token` split + `block_terminates` unification, `Operator` → `BinOp` /
> `LogicalOp` decoupling shared with IR/backend, parser EOF/`Error`-payload
> hardening, coercion/assignment validation unification (see below).

## Current state (verified 2026-09-18)

- Binary-precedence parsers delegate to `parse_binary(ops, operand)`
  (`parser/binary.rs`) with thin named wrappers; `cast.rs` stays separate.
- `atom.rs` is a 16-line dispatcher; `lexer.rs` `lex_token` is a dispatcher
  (`lex_operator/slash/dot/punctuation/string/char/numeric/identifier_or_keyword`).
- `TypedExprKind::Binary` holds `BinOp`, `ShortCircuit` holds `LogicalOp`;
  single mapping via `BinOp::from_syntax` in `analyze/expressions.rs`,
  re-exported by the IR (`ir/mod.rs`).
- §1 Unification is DONE: single `coerce_to(ty, expr) -> TypedExpr`
  (`analyze/expressions.rs:74`) handles `Never`/`Null`/numeric coercion,
  and `check_assignable(target, value)` (`:133`) is used by all four
  assignment forms — `Assign`, `FieldAssign`, `DerefAssign`, `IndexAssign`
  (`analyze/statements.rs:69,118,262,294`). `var_decl_to_typed` and
  `ArrayLiteral` go through `coerce_to` too; arity/shape logic is shared via
  `resolve_multi_value_types`; `validate_return_types`
  (`analyze/functions.rs:115`) reuses `check_assignable` so return
  strictness matches assignment strictness.

No remaining points. Strictness changes from the unification were locked
with failing-first negatives (mismatched field/deref/index assignment,
mixed-numeric `ArrayLiteral`) — see `analyze/test.rs:1100`.