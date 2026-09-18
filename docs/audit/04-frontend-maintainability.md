# Frontend Maintainability: Lexer, Parser, AST, Analyze

## Current state (verified)

- `src/frontend/lexer.rs:62-562`: `lex_token:62-341` is one giant `match` (~20 operator arms + literals); `lex_char_literal:343-443` handles `x`/`u{}` escapes with deep nesting; `lex_numeric:445-516` handles bases + floats. Good: never panics, emits `TokenKind::Error/Unknown` (`token.rs:20`).
- `src/frontend/parser.rs:1-234` is clean (`ParseError:42-48`, `fill_lookahead/peek/next:114-141`, `expect_*:144-179`). Top level only parses `import/fn/extern/type`.
- `src/frontend/parser/`: ~25 files. 11 binary-precedence leaves are near-clones differing only in operator set + callee: `additive.rs:13`, `term.rs:13`, `shift.rs:13`, `comparison.rs:13`, `equality.rs:14`, `bitwise_and.rs:12`, `bitwise_or.rs:12`, `bitwise_xor.rs:12`, `logical_and.rs:12`, `logical_or.rs:12`, `cast.rs:13`. Hottest file `atom.rs:13-302` is self-flagged `TODO: atom is too loaded at :14` (literals, `::`, struct literals `:52-80`, calls, `.field/& .*/.[i]`, arrays, grouping). Dispatcher `statement.rs:27-213`.
- `src/frontend/ast/`: 17 tiny pure-data files (6-78 lines) — healthy.
- `src/frontend/typed_ast.rs:1-436`: `Ty:155`, `SymbolTable:52` (`enter/exit_scope:72,76`, `lookup:97`), `TypedExprKind:236` still holds `Operator` (`TODO:250` to decouple into `Operation`).
- `src/frontend/analyze.rs:1-2115`: name resolution + type-check + generic monomorphization (`mangle/substitute/instantiate_generic:1698-1907`), `process_escape_sequences:2017`. Duplicated arity logic (`validate_multi_assignment_shape:565` vs `infer_multi_binding_types:586`), coercion logic (`coerce_expr_to_type:788` vs `coerce_or_alias:836` + inline `Never` fixups), immutability checks (`:238-253,:276-281,:543-559`).

## Opportunities

### 1. Generic binary-expression parser
Replace 11 files with one `parse_binary(&mut self, ops: &[Operator], next: fn) -> ParseResult<Expr>` (or a `macro_rules! binary_parser!`).

Trade-offs:
- **Pro:** ~300 lines -> ~40; precedence fix in one place; adding `**` or `??` later is trivial.
- **Con:** generic fn pointers / closures obscure the call graph; stack traces and `grep parse_bitwise_and` get harder. Error messages like `expected bitwise-or operand` need explicit `what` param or they regress to generic text.
- **Recommendation:** do it, but keep thin named wrappers (`parse_additive()` calls `parse_binary(...)`) so existing tests/greps keep working. This preserves readability while removing duplication.

### 2. Split `atom.rs` and `lexer.rs`
- `atom.rs` -> `literal.rs`, `call.rs`, `access.rs` (field/deref/index), `struct_literal.rs`, `primary.rs`. `lex_token` -> `lex_operator`, `lex_literal`, `lex_punct`.
- `typed_ast.rs:409-435` `then_branch_terminates` vs `else_branch_terminates` -> single `block_terminates(&[TypedStatement])`.

Trade-offs:
- **Pro:** smaller files review faster, merge conflicts drop.
- **Con:** Rust module churn; `use` imports shuffle. Do after (1), one file at a time, with `cargo test` green between moves. Do not split just for line count — split on responsibility boundaries.

### 3. Decouple `Operator` from `TypedExprKind::Binary`
`typed_ast.rs:250` already notes this. Introduce `enum BinOp { Add, Sub, ... }` distinct from syntax `Operator`, map once in `analyze::expr_to_typed:903`.

Trade-offs:
- **Pro:** backend/IR (`ir::BinOp:36`) can share one enum; desugaring (`a += b` -> `a = a + b`) lives in one place.
- **Con:** extra mapping code + migration of `codegen_expr:426` matches. Worth it only if you proceed with IR work (file 01); otherwise defer.

### 4. Unify coercion and assignment validation
Single `coerce_to(ty, expr) -> TypedExpr` handling `Never/Null/numeric` + single `check_assignable(target, value)` used by `Assign/FieldAssign/IndexAssign/DerefAssign`.

Trade-offs:
- **Strictness risk** (see file 02): unifying exposes inconsistencies (e.g. `ArrayLiteral` exact-match vs `var_decl` cast-insertion). Write the failing tests first, then unify — otherwise you silently change language semantics.

### 5. Harden parser error paths without full recovery
Fix `import_declaration.rs:31,77` and `switch.rs:79` `unwrap()` on `next/peek` -> `ok_or_else(|| self.error("unexpected EOF"))`. Propagate lexer `Error(token)` payload instead of `cannot start expression`.

Trade-offs: near-zero risk, immediate UX win. Full error-recovery (parse multiple errors per file) is explicitly NOT recommended yet — it complicates every `parse_*` signature for marginal benefit at this language stage.
