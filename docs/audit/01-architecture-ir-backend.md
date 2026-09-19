# Architecture, IR, and Backend

> Note (2026-09-18): addressed items removed. This file now lists only
> truly-remaining points. Removed: empty `src/backend/interpreter/` decision
> (deleted); §4 unsigned-integer execution tests (added
> `samples/unsigned_ops.fib` + `e2e_unsigned_ops`, values > `iN::MAX` for
> div/rem/cmp/shr — see `tests/e2e.rs:101`); §1 driver cutover to the IR
> middle-end; §2 `FunctionLowering` refactor (DONE, see below); §3
> control-flow dedup helpers (DONE, see below).

## Current state (verified 2026-09-18)

- Driver cutover is LIVE: `driver::lower_to_llvm_ir` (`src/driver.rs:799`)
  tries `ir::lower_typed_program` then the `ir_lower` consumer
  (`backend::lowering::lower_ir`), and falls back to the direct
  `lowering::lower` path on *any* error. `IrProgram` is on the production
  path for every program that fits the core subset; aggregates, tuples,
  pointers, enum-payloads, referenced module-`const` globals, and
  multi-value returns fall back to the direct path.
- `lower_typed_program` covers locals, integer/bool/float arithmetic,
  calls, `if`/`for`/`break`/`continue`, inline `defer`, `return`,
  assignment forms (`MultiAssign`/`MultiBinding`/`FieldAssign`/
  `DerefAssign`/`IndexAssign`), plain-enum `switch`, and module-`const`
  globals (`src/ir/mod.rs:8-17`). Anything else returns
  `LowerError::Unsupported` and the driver falls back.
- The `IrProgram` consumer (`src/backend/lowering/ir_lower.rs`, no longer
  `#[allow(dead_code)]`) handles every instruction the importer can emit
  except referenced module-`const` globals and multi-value `return` — both
  error and therefore trigger the driver fallback (never silently
  miscompiles).
- **§2 DONE — `FunctionLowering` owns the lowering state.**
  `src/backend/lowering/context.rs` now defines the struct holding `ctx +
  fn_ctx + function + vars + scope + deferred_stack + loop_ctx`
  (`context.rs:38-`, no more `#[allow(dead_code)]`, marker comment
  removed). Methods: `new`, `insert_block(&self, what)` (fn-name-aware),
  `parent_function`, `loop_ctx`, `enter_loop`/`exit_loop` (all
  `pub(super)`), `emit_deferred_frame`, `emit_frames_from(stack, from)`.
  `expr.rs` (`codegen_expr`/`compute_lvalue_ptr`/`store_lvalue`/
  `build_tuple_value`), `statements.rs` (`codegen_stmt`), and
  `llvm_lower.rs` (per-function construction, void-return tail via a
  cloned `deferred_stack` + `emit_frames_from`) are converted; the
  module-`const` branch builds its own `FunctionLowering`. Free helpers
  kept: `insert_block` (still used by `ir_lower.rs:354` + `test.rs`),
  `unpack_tuple_value`, `get_or_declare`, `call_result`,
  `coerce_int_to_llvm_type`, `map_type_to_llvm`, `create_entry_allocas`.
- **§3 DONE — control-flow scaffolding deduplicated.** New `FunctionLowering`
  helpers in `statements.rs`: `emit_sequence_with_fallthrough(bb, site,
  frame_name, stmts, succ)` (push deferred frame → emit stmts → pop → only
  if no terminator, `emit_deferred_frame` + branch to `succ`) now serves
  `if`-then/else, `switch` arms + wildcard default, and `for` bodies;
  `emit_loop(init, cond, post, body)` owns the whole `for` arm (loop ctx,
  cond/body/post/after blocks) — `for` post intentionally runs with the
  *enclosing* deferred frame, preserving previous semantics exactly.
  Short-circuit moved to `emit_short_circuit(op, left, right)` in
  `expressions.rs` (rhs/merge blocks + `phi`). All helpers return `Result`
  and never panic on a missing insert block. No behavior change: 269 lib +
  15 e2e + 3 probe tests green, clippy `-D warnings` clean, `cargo fmt`
  clean (verified 2026-09-18).
- Unsigned codegen is correct in both paths via `ty_is_unsigned`
  (`src/ir/mod.rs:266`): direct path uses
  `build_int_unsigned_div/rem`, `UGT/UGE/ULT/ULE`, logical `LShr`
  (`src/backend/lowering/expressions.rs:322-331,381-430,457`); IR path
  mirrors it (`src/backend/lowering/ir_lower.rs:525-567,637-757`) and
  `e2e_unsigned_ops` executes values above `iN::MAX`.

## Remaining points

### 5. Full structured diagnostics (scoped: see audit 02)

02 considers a `CompilerError { kind, line, col, hint }` across frontend +
backend. Deliberately deferred until the `FunctionLowering`/IR churn
settled. See `docs/audit/02-structured-diagnostics.md` for the scoping
decision.

### 6. Release/versioning (scoped: see audit 06)

06§2 covers how compiled `out/` binaries are distributed (nightly local
builds vs `cargo install --git` vs tagged GitHub Releases). See
`docs/audit/06-testing-tooling-release.md`.