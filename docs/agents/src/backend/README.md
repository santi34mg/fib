# `src/backend/`

`src/backend/mod.rs` exposes the LLVM lowering module. All implementation is
gated by the `llvm` Cargo feature.

See [`lowering/`](lowering/README.md) for both code-generation routes, ABI
assumptions, and support gaps.

The backend should consume already valid semantic input and return structured
errors. It should not decide source-language legality, print diagnostics, or
silently continue after LLVM builder failures.
