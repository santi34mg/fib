# Custom Compiler Backend: SSA + Sea of Nodes

## Context

Fiber currently lowers HIR directly to LLVM IR via inkwell, then shells out to clang-17 for native code. This plan replaces that dependency with a custom backend that uses SSA form as its initial IR and Sea of Nodes as its optimizing IR, ultimately emitting x86-64 assembly.

**New pipeline:**
```
HIR → SSA IR (basic blocks, block params) → Sea of Nodes (graph IR) → Scheduling → Instruction Selection → Register Allocation → Assembly → Assembler/Linker → Binary
```

The path is **HIR → SSA → Sea of Nodes** (not direct). SSA construction from structured HIR is mechanical; Sea of Nodes is then built by dissolving the CFG into a graph.

---

## Part 1: Project-Wide Changes (High Level)

### New module: `fibc/src/backend/`
Feature-gated behind `#[cfg(feature = "native")]` in `lib.rs`. Structure:

```
backend/
  mod.rs              -- entry: fn compile(CompilationUnit, &str) -> Result<Vec<u8>>
  ssa/
    mod.rs
    types.rs          -- SSA data structures
    builder.rs        -- HIR → SSA lowering
    opts.rs           -- mem2reg, simple opts
  sea/
    mod.rs
    types.rs          -- Sea of Nodes data structures
    builder.rs        -- SSA → Sea of Nodes conversion
    opts.rs           -- GVN, DCE, constant folding
    schedule.rs       -- Sea → linear block order
  codegen/
    mod.rs
    target.rs         -- x86-64 register defs, calling conventions
    isel.rs           -- instruction selection
    regalloc.rs       -- linear scan register allocation
    emit.rs           -- x86-64 assembly text emission
```

### Changes to existing files

- **`fibc/Cargo.toml`**: Add `native` feature (default disabled). No new external dependencies needed for assembly output.
- **`fibc/src/lib.rs`**: Add `#[cfg(feature = "native")] mod backend;`
- **`fibc/src/driver.rs`**: Add `#[cfg(feature = "native")]` compile path that calls `backend::compile` instead of LLVM lower + clang. Writes `.s` file, invokes system assembler (`as`) and linker (`ld` or `cc`).
- **`fibc/src/hir.rs`**: Optional — add `BindingId(u32)` to `HIRBinding` and assign statements for easier SSA variable tracking. Add `CallingConvention` enum to `HIRFunction` for ABI specification.

---

## Part 2: SSA IR — Detailed Design

### Data Structures (`backend/ssa/types.rs`)

**SSAFunction**: name, params (as SSAValues with types), return_type, ordered list of BasicBlocks, value counter.

**BasicBlock**: BlockId, block parameters (list of SSAValues — replaces phi nodes), instruction list, terminator.

**SSAValue(u32)**: Unique ID for every computed value. This is the "version" in SSA.

**SSAType**: Flattened from HIRTypeKind — `I8, I16, I32, I64, F32, F64, Bool, Ptr, Void, Struct(Vec<SSAType>), Array(SSAType, u64)`. No named types — all resolved.

**SSAInst**: Produces an optional SSAValue. Operations:
- Constants: `Const(Immediate)` — int, float, bool, null
- Arithmetic: `Add, Sub, Mul, Div, Mod` (each with two SSAValue inputs)
- Comparison: `ICmp(cmp_kind, a, b)`, `FCmp(cmp_kind, a, b)`
- Bitwise: `And, Or, Xor, Shl, Shr`
- Memory: `StackAlloc(SSAType)`, `Load(ptr)`, `Store(ptr, val)`, `GetFieldPtr(base, field_index)`, `GetElementPtr(base, index)`
- Calls: `Call(name, args)` — name is a string (mangled for cross-module)
- Cast: `Cast(val, target_type)`
- Null: `NullPtr`

**Terminator**:
- `Return(Option<SSAValue>)`
- `Jump(BlockId, Vec<SSAValue>)` — unconditional branch with block param args
- `Branch(cond, true_block, true_args, false_block, false_args)` — conditional

**Design choice — block parameters over phi nodes**: Block params and phis are isomorphic. Block params are easier to construct incrementally and maintain through transforms. The Sea of Nodes layer uses explicit Phi nodes (as Click's formulation requires).

### HIR-to-SSA Lowering (`backend/ssa/builder.rs`)

Process one HIRFunction at a time. Use the "alloca-then-mem2reg" strategy (same as LLVM):

**Initial lowering** (all variables are stack slots):
1. Create entry block. Map function params to SSAValues.
2. Walk HIRStmts sequentially, maintaining `current_block` and `var_map: HashMap<Identifier, SSAValue>` (maps variable names to their alloca pointers).
3. Lowering rules for each HIRStmt:
   - **Binding**: `StackAlloc` the type, `Store` the init value (or zero-init), record alloca in var_map
   - **Assign**: Lower RHS, `Store` to var's alloca
   - **FieldAssign**: `GetFieldPtr` on object's alloca + field_index, `Store` RHS
   - **DerefAssign**: Lower pointer expr, `Store` RHS to it
   - **IndexAssign**: `GetElementPtr`, `Store`
   - **Expr**: Lower expression, discard result
   - **Return**: Lower optional expr, emit `Return` terminator, start new unreachable block
   - **If**: Create `then_bb`, optional `else_bb`, `merge_bb`. Lower condition, emit `Branch`. Process each arm. Non-terminating arms `Jump` to `merge_bb`. Set current to `merge_bb`.
   - **For**: Create `cond_bb, body_bb, post_bb, exit_bb`. Lower init in current block. `Jump` to `cond_bb`. In cond: lower condition, `Branch` to body/exit. Body → post → cond cycle. Push LoopContext for break/continue.
   - **Break**: Emit deferred stmts, `Jump` to exit_bb
   - **Continue**: Emit deferred stmts, `Jump` to post_bb (or cond_bb if no post)
   - **Defer**: Push onto a defer stack. Before every Return and at function end (void), emit deferred stmts in LIFO order.

4. Expression lowering (recursive, each HIRExpression → SSAValue):
   - Literals → `Const`
   - Identifier → `Load` from var's alloca
   - Binary → lower both sides, emit op
   - Call → lower args, emit `Call`
   - FieldAccess → `GetFieldPtr` + `Load`
   - StructConstruct → `StackAlloc` struct, `GetFieldPtr` + `Store` per field, return alloca
   - AddressOf → return variable's alloca pointer directly
   - Deref → `Load`
   - Cast → `Cast`
   - IndexAccess → `GetElementPtr` + `Load`
   - ArrayLiteral → `StackAlloc` array, store each element

### mem2reg Pass (`backend/ssa/opts.rs`)

Promotes alloca/load/store patterns to direct SSA values with block parameters:

1. Identify promotable allocas: only used by `Load` and `Store` (address never escapes via `Call` args, `GetFieldPtr`, etc.)
2. Compute dominator tree and dominance frontiers for the CFG
3. Place block parameters at dominance frontiers of blocks containing stores
4. Rename: walk dominator tree, replacing `Load`s with the reaching definition and `Store`s with updates to the reaching definition. At block transitions, pass the current definition as a block parameter argument.
5. Remove dead alloca/load/store instructions

After mem2reg, the IR is in proper SSA form with block parameters at join points.

---

## Part 3: Sea of Nodes — Detailed Design

### Data Structures (`backend/sea/types.rs`)

**NodeId(u32)**: Index into node arena.

**Node**: id, op (NodeOp), inputs (Vec<NodeId>), outputs (Vec<NodeId> — reverse edges), ty (SSAType).

**NodeOp categories**:
- **Control**: `Start`, `End`, `Region(n)`, `Loop`, `If`, `IfTrue`, `IfFalse`, `Return`
- **Data**: `Parameter(idx)`, `Const(Immediate)`, `Phi(n)`, `Add`, `Sub`, `Mul`, `Div`, `ICmp(cmp)`, `FCmp(cmp)`, `And`, `Or`, `Xor`, `Shl`, `Shr`, `Call(name)`, `Cast(type)`
- **Memory**: `Load`, `Store`, `Alloc`, `GetFieldPtr(idx)`, `GetElementPtr`
- **Projection**: `Proj(idx)` — extracts component from multi-result node

**SeaGraph**: arena of Nodes, start NodeId, end NodeId.

**Edge conventions** (via `inputs` list):
- `Region`: all inputs are control edges from predecessors
- `Phi`: input[0] = owning Region, input[1..n] = data values matching Region's control inputs
- `If`: input[0] = control, input[1] = condition
- Data ops (`Add` etc.): inputs are operand NodeIds
- `Load`: input[0] = control, input[1] = memory state, input[2] = pointer
- `Store`: input[0] = control, input[1] = memory state, input[2] = pointer, input[3] = value
- `Return`: input[0] = control, input[1] = memory state, input[2] = optional return value

**Memory state threading**: A "memory token" value flows through the graph. `Start` produces the initial token. Each `Store`/`Load`/`Call` consumes the previous token and produces a new one. At merge points (Region), `Phi` nodes merge memory tokens. This serializes memory operations.

### SSA-to-Sea Conversion (`backend/sea/builder.rs`)

1. Create `Start` and `End` nodes. Create `Parameter` nodes off Start for each function param.
2. For each BasicBlock, create a `Region` node (or `Loop` for loop headers detected by back-edges).
3. For each block parameter, create a `Phi` node attached to its Region.
4. For each SSA instruction, create the corresponding data/memory node. Resolve inputs via `value_to_node: HashMap<SSAValue, NodeId>`.
5. For each terminator:
   - `Jump(target, args)`: wire current block's control to target Region. Wire args to target's Phi nodes.
   - `Branch(cond, ...)`: create `If` node (control + condition), create `IfTrue`/`IfFalse` projections. Wire to respective Regions.
   - `Return(val)`: create `Return` node, wire to `End`.
6. **Float pure nodes**: Remove control input from any pure data node (no side effects). It can now be scheduled anywhere valid.

### Sea of Nodes Optimizations (`backend/sea/opts.rs`)

- **Global Value Numbering (GVN)**: Hash each node by (op, inputs). Deduplicate identical computations. Trivial in Sea of Nodes — no block boundaries to consider.
- **Constant Folding**: If all inputs to a pure node are `Const`, replace with a single `Const`.
- **Dead Code Elimination**: Remove nodes with no outputs (uses) and no side effects. Propagate transitively.
- **Strength Reduction**: Pattern match: `x * 2^n` → `x << n`, `x * 1` → `x`, `x + 0` → `x`, etc.
- **Loop-Invariant Code Motion**: Happens automatically via scheduling — floating nodes whose inputs are outside a loop get scheduled outside it.

### Scheduling (`backend/sea/schedule.rs`)

Converts the Sea of Nodes graph back to a linear block ordering:

1. **Early schedule**: For each node, compute earliest legal position = deepest Region dominating all its inputs.
2. **Late schedule**: For each node, compute latest legal position = shallowest Region dominated by all its uses.
3. **Final placement**: For each node, choose between early and late. Heuristic: pick the block with lowest loop nesting depth (hoists out of loops), break ties by choosing latest (reduces register pressure).
4. **Block ordering**: Topological sort Region nodes by control flow.
5. **Intra-block ordering**: Topological sort nodes within each block by data dependencies.

Output: A sequence of basic blocks with ordered instructions — essentially SSA again, but optimized. Feed into instruction selection.

---

## Part 4: Code Generation

### Target Description (`backend/codegen/target.rs`)

x86-64 System V ABI:
- 14 GP registers: rax, rcx, rdx, rbx, rsi, rdi, r8–r15 (excluding rsp, rbp)
- Integer arg passing: rdi, rsi, rdx, rcx, r8, r9 (then stack)
- Return value: rax
- Caller-saved: rax, rcx, rdx, rsi, rdi, r8–r11
- Callee-saved: rbx, r12–r15, rbp

Define `MachInst` enum for x86-64 instructions and `MachOperand` for `Reg(VReg)`, `Imm(i64)`, `Mem(base, offset)`, `Label(String)`.

### Instruction Selection (`backend/codegen/isel.rs`)

Tree-pattern matching on scheduled nodes. Walk each block's nodes in order:

- `Const(int)` → `mov vreg, imm`
- `Add(a, b)` → `add va, vb` (or `add va, imm` if b is Const)
- `ICmp + If` → fuse into `cmp` + `jcc`
- `Load(ptr)` → `mov vreg, [ptr_vreg]`
- `Store(ptr, val)` → `mov [ptr_vreg], val_vreg`
- `GetFieldPtr(base, idx)` → `lea vreg, [base + offset]` (compute from struct layout)
- `GetElementPtr(base, idx)` → `lea` or `imul + add` for index × element_size
- `Call(name, args)` → move args to ABI registers, `call label`, result in rax
- `Return(val)` → `mov rax, val`, `ret`
- Block params → parallel moves at block boundaries (resolved in regalloc)

Need a **LayoutComputer** for struct/array layout: size, alignment, field offsets. Cache in a `TypeLayouts` map.

### Register Allocation (`backend/codegen/regalloc.rs`)

Linear scan algorithm:

1. Number all MachInsts sequentially. Compute live intervals for each VReg (definition point to last use point).
2. Sort intervals by start point.
3. Walk intervals. Maintain active set of intervals assigned to physical registers.
   - Expire ended intervals, free their registers.
   - If free register available, assign it.
   - If not, spill: pick the interval with longest remaining range. Insert spill (store to stack) and reload (load from stack) instructions.
4. Handle calling convention: before `call`, spill all live caller-saved registers. After `call`, reload them.
5. Resolve parallel moves for block parameter transitions using a cycle-breaking algorithm (temp register or stack slot).

### Assembly Emission (`backend/codegen/emit.rs`)

Emit x86-64 AT&T syntax assembly:
- `.globl` directives for exported functions
- Function labels and instruction text
- `.data` section for string literals and global constants
- Link against libc for extern functions (printf, malloc, etc.)

Invoke system assembler (`as`) and linker (`cc -o output`) from the driver, similar to how clang is invoked today.

---

## Part 5: Implementation Phases

**Phase 1 — SSA IR**: Define types, implement HIR→SSA builder, implement mem2reg. Verify with pretty-printer.

**Phase 2 — Sea of Nodes**: Define types, implement SSA→Sea conversion, implement GVN/DCE, implement scheduling. Verify round-trip equivalence.

**Phase 3 — Codegen**: Define MachInst/target, implement isel, implement regalloc, implement asm emitter. Wire into driver.

**Phase 4 — Hardening**: End-to-end tests against sample programs. Add optimizations incrementally. Eventually replace text asm with direct ELF emission.

## Verification

- **Unit tests**: Each phase has its own tests (SSA builder output, Sea graph structure, scheduling correctness, isel patterns)
- **End-to-end**: Compile samples/ programs with both LLVM and native backends, diff execution output
- **Run**: `cargo test -p fibc --features native`
- **Manual**: `fiber compile` with a flag to select native backend, run produced binary

## Critical Files

| File | Change |
|------|--------|
| `fibc/Cargo.toml` | Add `native` feature |
| `fibc/src/lib.rs` | Add `mod backend` behind feature gate |
| `fibc/src/hir.rs` | Optional: add BindingId, CallingConvention |
| `fibc/src/driver.rs` | Add native compile path |
| `fibc/src/backend/**` | All new — the entire backend |
