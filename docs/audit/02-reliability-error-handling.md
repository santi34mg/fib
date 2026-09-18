# Reliability and Error Handling

## Current state (verified)

- `rg` finds ~30 non-test `unwrap/expect/unreachable`:
  - `src/backend/lowering/llvm_lower.rs:66,449,458,499,501,502,516,1329,1338,1634,1664,1700,1721,1797,1993,2033` — mostly `get_insert_block().unwrap()`, `get_nth_param().unwrap()`, `build_*.unwrap()`, plus `expect("deferred stack.../then frame/else frame/loop body...")`.
  - `src/frontend/analyze.rs:79` (`import.path.last().unwrap()`), `:667` (`unreachable!("rejected above")`).
  - `src/frontend/typed_ast.rs:92` (`.expect("SymbolTable has no scopes")`), `:77` (`assert!(scopes.len()>1)`).
  - `src/frontend/parser/import_declaration.rs:31,77`, `switch.rs:79` (`.unwrap()` on `next/peek`), `pointer.rs:32` (`unreachable!()`).
- Error types: `AnalysisError{msg,line}` (`src/frontend/analyze.rs:21-46`) loses `line` via `From<String>`; only `stmt_to_typed:222-225` attaches it. Backend returns `Box<dyn Error>` (`llvm_lower.rs:37`) with `format!.into()` and no spans.
- Soundness gaps: `DerefAssign` (`analyze.rs:408-428`) discards `pointee_ty` with `let _ =`; `FieldAssign` (`:267-306`), `IndexAssign` (`:429-451`), `IndexAccess` (`:1420-1440`) skip value/element coercion and index-is-int checks; `ArrayLiteral` (`:1442-1471`) requires exact `==` homogeneity while `var_decl_to_typed:719-728` inserts numeric `Cast` elsewhere.
- `src/main.rs:1-6` silently does nothing without `llvm` feature. `map_type_to_llvm` failure on params (`llvm_lower.rs:66`) panics while return type (`:77-81`) correctly uses `?`.
- Lexer never panics (good): emits `TokenKind::Error/Unknown` (`token.rs:20`, `lexer.rs:304,320,348...`), but only `parser/statement.rs:195-197` surfaces the message; other paths degrade to generic `cannot start/mismatch`.

## Opportunities

### 1. Replace panics with `Result` in lowering
Change `get_insert_block().unwrap()` / `get_first_basic_block().unwrap()` / `expect("defer frame")` to return a `LowerError::MissingBlock{what, fn_name, line}`.

Trade-offs:
- **Pro:** compiler bugs become actionable errors instead of `panicked at llvm_lower.rs:1634`. Essential once users compile untrusted code.
- **Con:** verbose. Mitigate with `ok_or_else(|| LowerError::...)` + `?`, and a `FunctionLowering::insert_block()` helper that adds context once.
- Do NOT just `#[deny(clippy::unwrap_used)]` globally on day one — you will drown in test `expect`s. Scope it: `clippy::unwrap_used` deny for `src/backend`, allow for `#[cfg(test)]`.

### 2. Structured diagnostics with spans
Introduce `CompilerError{kind, line, col, hint}` or `thiserror` + `miette`-style rendering. Thread `line` from `TypedExpr/Statement` (you already have it in places) through `analyze` and `lower`.

Trade-offs:
- **Pro:** `AnalysisError at line X` everywhere instead of only statement-level.
- **Con:** requires touching every `From<String>` callsite; invasive. Intermediate step: keep `AnalysisError` but add `with_line()` at `resolve_declaration:107`, `signature_to_typed:100`, import errors `:62-63`, and stop using bare `format!` without line.

### 3. Close the assignment soundness holes
Make `DerefAssign` coerce RHS to pointee type, `FieldAssign` coerce to field type, `IndexAssign` check index is integer + value matches element type. Unify with existing `Assign:254-261` logic.

Trade-offs:
- **Strictness vs. ergonomics.** Stricter checks break currently-compiling (but wrong) programs. That is desirable pre-1.0, but document as breaking change and add negative tests first.
- `ArrayLiteral` homogeneity (`:1442-1471`) is the opposite tension: exact `==` rejects `[@int(1), @int8(2)]` that `var_decl` would accept via cast. Decide one numeric coercion rule and reuse `coerce_expr_to_type:788` everywhere instead of three ad-hoc `Never -> ty` fixups (`:674-675,:796-799,:836-852`).

### 4. Fix the small panics first (quick wins)
- `analyze.rs:79`: `import.path.last().unwrap()` -> `ok_or("empty import path")`.
- `typed_ast.rs:321` (`&self.values[0]` in `Deref for TypedReturn`): panics on empty `return;`. Return `Option` or handle empty.
- `typed_ast.rs:77,92`: `assert/expect` on scope stack -> return `Result` or `debug_assert` + graceful error.
- `llvm_lower.rs:66`: `.unwrap()` -> `?` (one-line fix, inconsistent with `:77`).
- `llvm_lower.rs:1213` `.unwrap_or(0)` for size/align: emit explicit `UnknownLayout` error instead of silently generating wrong code.

### 5. Lexer error propagation
Propagate `TokenKind::Error(String)` message in `atom.rs` / `type_expression.rs` instead of generic errors.

Trade-off: slightly more parser code, much better UX for typos like `@nope` or unterminated strings. Low risk.
