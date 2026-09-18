# Architecture, IR, and Backend

> Note (2026-09-18): addressed items removed. This file now lists only
> partially addressed or unaddressed points. Removed: empty
> `src/backend/interpreter/` decision (deleted).

## Current state (verified 2026-09-18)

- `src/ir/` is now split (`mod.rs`, `builder.rs`, `expressions.rs`, `statements.rs`, `display.rs`, `test.rs`). `lower_typed_program(TypedProgram) -> Result<IrProgram, LowerError>` exists (`src/ir/mod.rs:274-295`) but covers a core subset only — header at `:8-15` still says the backend consumes typed AST directly; `MultiAssign/MultiBinding/FieldAssign/DerefAssign/IndexAssign/Switch/Const` return `LowerError::Unsupported` (`src/ir/statements.rs:265-283`, `src/ir/mod.rs:283-287`).
- Production path is unchanged: `src/backend/lowering/llvm_lower.rs:19` `lower(TypedProgram, …)` called from `src/driver.rs:631-632`. The `IrProgram` consumer is parallel dead code: `src/backend/lowering/ir_lower.rs:28` `lower_ir` (`#[allow(dead_code)]`), exported but never called from the driver.
- `src/ir/test.rs:23-136` has 8 tests (straight-line, `if+for` → `if/goto`, `&&`, `defer`, `break/continue`, `SymbolId` shadowing, extern, `switch/struct` → `Err`).
- `src/backend/lowering/` is split (`llvm_lower.rs:202` lines + `context.rs`, `expressions.rs:901`, `statements.rs:658`, `types.rs`, `ir_lower.rs:865`, `mod.rs`). No `defer.rs`; defer helpers live in `context.rs:182-209`.
- `FunctionLowering` (`src/backend/lowering/context.rs:38-49`) is a `#[allow(dead_code)]` stub — defined but never constructed; lowering still threads `(ctx, vars, scope, loop_ctx, deferred_stack)` tuples (e.g. `statements.rs:21-28`).
- Unsigned codegen is correct in both paths via `ty_is_unsigned` (`src/ir/mod.rs:246-258`): direct path uses `build_int_unsigned_div/rem`, `UGT/UGE/ULT/ULE`, `build_right_shift(..., !is_unsigned)` (`src/backend/lowering/expressions.rs:322-331,381-430,457`); IR path mirrors it (`src/backend/lowering/ir_lower.rs:525-567,637-757`).

## Remaining points

### 1. Finish the IR middle-end [PARTIALLY ADDRESSED]
Remaining: extend `lower_typed_program` beyond the core subset (assignment forms, `switch`, consts), then cut `driver` over to `lower_ir` and delete/flag-gate the direct path.

What it unlocks:
- A testable, backend-independent place for desugaring, constant folding, dead-code elimination, borrow/move checks later.
- LLVM lowering becomes mostly mechanical `BasicBlock -> append_basic_block`.
- Future custom backend reuses the same IR.

Trade-offs:
- **Double lowering bugs.** During migration you maintain two paths. Needs a flag or parallel e2e tests to avoid drift.
- **Design lock-in.** `Operand::Place(String)` names bindings by string; you will want `SymbolId`s eventually (IR tests already use shadowing `SymbolId`s — propagate that into the real type).

### 2. Finish the `FunctionLowering` refactor [PARTIALLY ADDRESSED]
File split is done; the struct migration is not. Remaining: make `FunctionLowering` own `ctx + vars + scope + loop_ctx + deferred_stack`, move `insert_block()` (`context.rs:74-81`) onto it, and convert `expressions.rs` / `statements.rs` / `ir_lower.rs` call sites off the tuples.

Trade-offs:
- **Pro:** reviewability, parallel work, easier to forbid `unwrap` per module.
- **Con:** inkwell lifetimes make the conversion fiddly; do it in one focused refactor with `cargo test` green, not interleaved with IR migration.

### 3. Deduplicate control-flow scaffolding [NOT ADDRESSED]
`statements.rs:326,331,336,392-395,497,512` repeats `append_basic_block`; `:347,362,376,407,420,443,463` repeats `position_at_end`; short-circuit `rhs_bb/merge_bb + phi` is inline in `expressions.rs:279-295`. Only `insert_block/parent_function` (`context.rs:74-91`) and defer-frame emitters exist — no `emit_if / emit_short_circuit / emit_loop`.

**Opportunity:** helpers like `emit_if(cond, then_fn, else_fn)`, `emit_short_circuit(op,lhs,rhs)`, `emit_loop(...)`.

Trade-offs:
- **Pro:** fixes `defer + break/continue` consistently in one place.
- **Con:** over-abstraction hides LLVM block ordering bugs. Keep helpers small and return `Result`, never panic on missing insert block.

### 4. Unsigned integers: execution tests [PARTIALLY ADDRESSED]
Codegen is fixed; tests are not per the audit bar (`values > iN::MAX` executed). Current coverage is only `src/backend/lowering/test.rs:111-129` (`uint8 = 200`, asserts IR string contains `udiv/ugt`) plus `zext/sext` (`:34-73`). No `URem/ULT/ULE/UGE/LShr` execution, no e2e uint case.

Remaining: add execution tests (via e2e sample or backend `Context::create` JIT/run) with values `> iN::MAX` for `div/rem/cmp/shr`. Trade-off: small churn, high correctness value — do early before stdlib depends on wraparound behavior.
