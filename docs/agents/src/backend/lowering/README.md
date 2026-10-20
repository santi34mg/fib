# Backend lowering boundaries

`ir_lower::verify_input` invokes the shared verifier and returns a borrowed proof.
`abi::classify_abis` builds descriptions keyed by function identity, dispatching
on each signature's convention with resolved target/layout and logical IR types.
Production C supports 32-bit integer/void boundaries; Fib is unsupported.

`llvm_lower::emit_llvm(ir, target)` remains the sole public emission boundary.
`llvm_abi` uses each function's description for declarations, incoming arguments,
calls to it and returns from it. The CFG emitter maps logical SSA values to LLVM
blocks/PHIs, traverses reverse postorder, and patches incoming edges after body
emission. Parallel same-target branch arms are split. Artifacts require LLVM
verification and can emit PIC objects. `host_target()` selects the host triple.

The separate `types::{resolve_target, plan_types}`, `context`, `expressions` and
`statements` planning APIs remain explicit-error sketches. Working emission uses
private state instead of claiming these general APIs are complete. Classification
and shared verification work without LLVM; emission requires the feature.

`next/ir_lower.rs` remains inactive legacy source and is not a fallback.
