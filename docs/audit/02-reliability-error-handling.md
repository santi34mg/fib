# Reliability and Error Handling

> Note (2026-09-18): addressed items removed. This file now lists only
> partially addressed points. Removed: assignment soundness holes (fixed in
> `analyze/statements.rs` + `analyze/expressions.rs`), quick-win panics
> (import path, `TypedReturn`, scope stack, param `unwrap`, `UnknownLayout`,
> parser `expect_*`), lexer `Error` propagation (`primary.rs`,
> `type_expression.rs`, `statement.rs`).

## Current state (verified 2026-09-18)

- Zero non-test `unwrap/expect/unreachable` in `src/backend` + `src/frontend` (`rg` clean; remaining hits are comments and `#[cfg(test)]` modules). `get_insert_block().unwrap()` is gone, replaced by `insert_block() -> Result<_, Box<dyn Error>>` (`src/backend/lowering/context.rs:74-81`) used in `expressions.rs`, `statements.rs`, `ir_lower.rs`.
- Error types are still untyped: `AnalysisError{msg, line: Option}` (`src/frontend/analyze.rs:19-25`, `From<String>` → `line: None`), backend still `Result<_, Box<dyn Error>>` with `format!.into()` and no spans (`llvm_lower.rs:19`, `ir_lower.rs:28`). Only `ir::LowerError::Unsupported` exists; there is no `LowerError::MissingBlock`.
- Only diagnostic improvement so far: `AnalysisError::with_line()` (`analyze.rs:47-55`) applied at `stmt_to_typed` (`analyze/statements.rs:27`), and `DriverError::Parse/Analysis/Lower…` (`driver.rs:150-295`) preserving `ParseError`.

## Remaining points

### 1. Replace panics with typed lowering errors [PARTIALLY ADDRESSED]
Panics are eliminated, but the audit asked for `LowerError::MissingBlock{what, fn_name, line}`. Current `insert_block()` error is a `format!("lowering '{}': no insert block")` string with no function name/line/span.

Remaining: introduce `LowerError` (`MissingBlock`, `UnknownLayout` is already an ad-hoc string in `types.rs:35-91` — fold it in) and convert `context.rs` + `expressions.rs` + `statements.rs` + `ir_lower.rs` off `Box<dyn Error>`.

Trade-offs:
- **Pro:** compiler bugs become actionable errors instead of opaque strings. Essential once users compile untrusted code.
- **Con:** verbose. Mitigate with `ok_or_else(|| LowerError::...)` + `?`, and the `FunctionLowering::insert_block()` helper adding context once.
- Do NOT just `#[deny(clippy::unwrap_used)]` globally on day one — you will drown in test `expect`s. Scope it: `clippy::unwrap_used` deny for `src/backend`, allow for `#[cfg(test)]`.

### 2. Structured diagnostics with spans [PARTIALLY ADDRESSED]
Still `AnalysisError{msg, line: Option}` + `Box<dyn Error>`; no `CompilerError{kind, line, col, hint}`, no `thiserror`/`miette`.

Remaining: thread `line` from `TypedExpr/Statement` through `analyze` and `lower`. Intermediate step (cheaper than full `miette`): keep `AnalysisError` but apply `with_line()` at `resolve_declaration`, signature lowering, and import errors, and stop using bare `format!` without line.

Trade-offs:
- **Pro:** `AnalysisError at line X` everywhere instead of only statement-level.
- **Con:** requires touching every `From<String>` callsite; invasive. Do the `with_line()` pass first, structured rendering later.
