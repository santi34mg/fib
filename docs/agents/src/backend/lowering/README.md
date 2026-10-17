# Backend lowering boundaries

Active modules under [`src/backend/lowering/`](../../../../../src/backend/lowering)
are architecture sketches. Operations fail with `BackendError::NotImplemented`,
not successful empty artifacts.

| File | Intended boundary |
| --- | --- |
| `ir_lower.rs` | `verify_input` -> opaque borrowed `VerifiedIr` |
| `types.rs` | explicit `TargetSpec`, target/layout resolution and type plan |
| `abi.rs` | shared function/call ABI planning |
| `context.rs` | symbol predeclarations, module/function emission plans |
| `expressions.rs` | instruction emission |
| `statements.rs` | block/terminator/edge emission |
| `llvm_lower.rs` | `emit_llvm` -> owned textual `LlvmArtifact` |
| `error.rs` | backend error contract |
| `test.rs` | explicit failure and absence of fabricated success |

Planned order: verify SSA input, resolve target/layout, map types, classify ABI,
predeclare all symbols, set up blocks/PHIs, emit bodies, patch edges, verify LLVM,
serialize text. None executes today. `TargetSpec` has triple/CPU/features and no
implicit host default. Artifact text/triple/layout are private and cannot be
constructed by an unfinished successful path. File output, optimization, object
production and linking are separate future concerns.

## Preserved but inactive legacy source

`next/ir_lower.rs` is not declared by the module tree. It references removed
frontend typed-AST and old flat-IR APIs; its Inkwell code is not compiled or a
working alternate backend. Do not use it as proof of current type layouts,
bounds checks, LLVM support, route parity or runtime behavior.

When implementing these boundaries, preserve verified-input lifetimes, explicit
IR identities and target-derived layouts. Do not repair malformed SSA silently,
reconstruct bindings from source names, auto-declare missing identities, print
and continue on builder failures, or bypass input/LLVM verification. Add real
verification and executable tests once emission is implemented.
