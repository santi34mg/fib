# Frontend Maintainability: Lexer, Parser, AST, Analyze

> Note (2026-09-18): addressed items removed. This file now lists only the
> unaddressed point. Removed: generic `parse_binary` with thin wrappers
> (`parser/binary.rs`), `atom.rs` split (`primary/access/call/struct_literal`)
> + `lex_token` split + `block_terminates` unification, `Operator` → `BinOp` /
> `LogicalOp` decoupling shared with IR/backend, parser EOF/`Error`-payload
> hardening.

## Current state (verified 2026-09-18)

- Binary-precedence parsers delegate to `parse_binary(ops, operand)` (`parser/binary.rs:16-20`) with thin named wrappers; `cast.rs` stays separate.
- `atom.rs` is a 16-line dispatcher; `lexer.rs:62-92` `lex_token` is a dispatcher (`lex_operator/slash/dot/punctuation/string/char/numeric/identifier_or_keyword`).
- `TypedExprKind::Binary` holds `BinOp`, `ShortCircuit` holds `LogicalOp` (`typed_ast.rs:247-264,325-337`); single mapping via `BinOp::from_syntax` in `analyze/expressions.rs:440-441`; IR re-exports it (`ir/mod.rs:60`).
- Coercion/assignment validation is still duplicated (see below).

## Remaining points

### 1. Unify coercion and assignment validation [NOT ADDRESSED]
Still `coerce_expr_to_type` (`analyze/expressions.rs:60`) + `coerce_or_alias` (`:104`) with no single `coerce_to`; still `validate_multi_assignment_shape` (`analyze/statements.rs:402`) vs `infer_multi_binding_types` (`:423`) duplicated arity logic. `Assign:58`, `FieldAssign:108`, `DerefAssign:240`, `IndexAssign:273` each inline their own `coerce_or_alias` + `found/map_err` block — no shared `check_assignable`. `var_decl_to_typed:477+` keeps inline `Never`/alias/array-size logic.

Remaining: introduce single `coerce_to(ty, expr) -> TypedExpr` handling `Never/Null/numeric` + single `check_assignable(target, value)` used by all four assignment forms.

Trade-offs:
- **Strictness risk** (see file 02): unifying exposes inconsistencies. Write the failing tests first (`get_typed_err` for mismatched field/deref/index assignment + mixed-numeric `ArrayLiteral`), then unify — otherwise you silently change language semantics.
