# Architecture, IR, and Backend

## Current state (verified)

- `src/ir/mod.rs:1-114` is a Dragon-style 3-address IR skeleton (`Label`, `Temp`, `Operand`, `BinOp`, `Instruction`, `BasicBlock`, `IrFunction`, `IrProgram`). Header at `:8-9` explicitly says `lower_typed_program is not implemented yet; backend::lowering still consumes typed AST directly`.
- `src/backend/lowering/llvm_lower.rs:1-2050` lowers `TypedProgram` straight to LLVM IR strings via `inkwell`. Entry `lower:37`, `codegen_expr:426`, `map_type_to_llvm:1216`, `create_entry_allocas:1321`, `codegen_stmt:1389`.
- `src/backend/mod.rs:1` only re-exports `lowering`. `src/backend/interpreter/` exists on disk but contains 0 files and is unreferenced.
- `src/backend/lowering/mod.rs:1-5` exports nothing when `llvm` feature is off.

This is a classic frontend-direct-to-backend shape: fast to evolve the language, but all control-flow lowering (`&&/||`, `if`, `for`, `switch`, `defer` + `break/continue`) lives inside LLVM builder calls.

## Points of interest

### 1. Finish the IR middle-end vs. keep direct lowering
**Opportunity:** implement `ir::lower_typed_program(TypedProgram) -> IrProgram`, then make `llvm_lower` consume `IrProgram`.

What it unlocks:
- A testable, backend-independent place for desugaring, constant folding, dead-code elimination, borrow/move checks later.
- LLVM lowering becomes mostly mechanical `BasicBlock -> append_basic_block`.
- Future custom backend (`README.md:11-12` mentions one) reuses the same IR.

Trade-offs:
- **Cost now vs. payoff later.** IR is only worth it if you plan >1 backend or IR-level opts. If LLVM is the only backend for the next 6-12 months, direct lowering is faster to iterate.
- **Double lowering bugs.** During migration you maintain two paths (typed AST -> LLVM and typed AST -> IR -> LLVM). Needs a flag or parallel e2e tests to avoid drift.
- **Design lock-in.** `Operand::Place(String)` (`src/ir/mod.rs:27`) names bindings by string; you will want `SymbolId`s eventually. If you ship string-based IR now, later passes bake in fragile lookups.

Suggested first step (low-risk): keep direct lowering, but add `ir::tests` that lower one function with `if` + `for` to IR and pretty-print it. Decide on `Temp`/`Label` allocation API before rewriting the backend.

### 2. Split the 2050-line `llvm_lower.rs`
**Opportunity:** extract `types.rs` (`map_type_to_llvm`, `coerce_int_to_llvm_type`), `expr.rs` (`codegen_expr`, `compute_lvalue_ptr`), `stmt.rs` (`codegen_stmt`, loop/switch helpers), `defer.rs` (deferred stack).

Trade-offs:
- **Pro:** reviewability, parallel work, faster `cargo check` incrementally, easier to forbid `unwrap` per module.
- **Con:** inkwell lifetimes (`CodegenCtx<'ctx,'r>`, `LoopContext<'ctx>`) make splits fiddly; you will pass `&CodegenCtx` + `&mut vars` + `deferred_stack` everywhere. Do the split after introducing a `FunctionLowering` struct that owns those, not before.

### 3. Deduplicate control-flow scaffolding
`llvm_lower.rs:496-519,1700-1874,1993-2037` repeats `get_insert_block/append_basic_block/position_at_end/phi`.

**Opportunity:** helpers like `emit_if(cond, then_fn, else_fn)`, `emit_short_circuit(op,lhs,rhs)`, `emit_loop(...)`.

Trade-offs:
- **Pro:** fixes `defer + break/continue` consistently in one place.
- **Con:** over-abstraction hides LLVM block ordering bugs. Keep helpers small and return `Result`, never panic on missing insert block.

### 4. Decide the fate of `src/backend/interpreter/`
Empty directory with no module reference is confusing to newcomers.

Options:
- A) Delete it until needed (recommended if no interpreter roadmap).
- B) Keep as `tree-walking interpreter over TypedProgram` for fast `fib run` without clang/LLVM. Useful for tests, but doubles semantic drift risk.

### 5. Unsigned integers
`llvm_lower.rs:1225` — `TODO: make unsigned truly unsigned`. Today `UInt*` lowers to signed `iN`, so `div/rem/cmp/shr` are wrong for large values.

Fix is localized (`UDiv vs SDiv`, `ICmpULT vs SLT`, `LShr`), but needs e2e tests with values `> iN::MAX`. Trade-off: small churn, high correctness value — do early before stdlib depends on wraparound behavior.
