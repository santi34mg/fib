# `src/lib.rs`

## Responsibility

The crate facade exposes all implementation modules and re-exports
`CompilerError`, `CompilationOptions`, `CompileOutput`, `DriverError`, and
`EmitKind`.

Public entry points:

- `compile_project`: calls `driver::compile` and returns a boxed
  `CompilerError` on failure.
- `check_project`: calls the frontend-only driver entry and returns
  `DriverError` directly.

## Current Contract

Frontend-only work is available without the `llvm` Cargo feature. Backend
emits require it. The API accepts an entry path plus options; despite the
function name, there is no project manifest or package graph abstraction.

## Known Risks

- The two entry points expose different public error types.
- Public `pub mod` declarations expose parser, AST, IR, backend, driver, and CLI
  internals as an accidental compatibility surface.
- `source_override` is mixed into production options primarily to support tests.
- Documentation in this file refers to `CompilerError::Toolchain`, but
  `Toolchain` is an `ErrorKind`, not a `CompilerError` enum variant.

## Improvement Direction

Define a deliberate API module with stable request, stage-output, and diagnostic
types. Consider a source-provider abstraction for in-memory compilation. Keep
internal compiler modules private unless external tooling has a concrete use
case and compatibility policy.
