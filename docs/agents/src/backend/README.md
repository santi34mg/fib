# Backend sketch

[`backend/mod.rs`](../../../../src/backend/mod.rs) exposes `lowering` without
feature gating. Active APIs have no LLVM objects and are available in
no-default-features builds. Cargo still enables optional Inkwell/LLVM 21 by
default; dependency requirements are distinct from implemented backend behavior.

Only self-owned SSA `Ir` is intended as input. No LLVM emission, direct typed-AST
route, fallback, object generation or linking is implemented.
See [lowering boundaries and source map](lowering/README.md).
