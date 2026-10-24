# Library facade

[`src/lib.rs`](../../../src/lib.rs) publicly exposes `backend`, `build_plan`,
`builder_object`, `cli`, `diagnostics`, `driver`, `frontend`, `ir`, and
`ir_interpreter`, and re-exports only `diagnostics::CompilerError`.

There are no `compile_project`, `check_project`, `CompilationOptions`,
`CompileOutput`, `DriverError`, or `EmitKind` APIs. Use direct tokenizer/parser
APIs for syntax work; see [parser](frontend/parser.md). Driver APIs support parsing/analysis and the scalar/control-flow executable subset. Public modules are an implementation
surface, not a guarantee of a stable or complete compiler API.
