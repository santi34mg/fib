# Library facade

[`src/lib.rs`](../../../src/lib.rs) keeps the `fibc` facade: `backend`, `builtins`,
`cli`, `diagnostics`, `driver`, `frontend`, `ir`, `plan`, `standard_library` and
`ir_interpreter`, plus `diagnostics::CompilerError`. Compiler stages are re-exported
from [workspace crates](../../../crates/README.md). `ir::lowering` re-exports
`fib-lowering`; the `fib-ir` package itself has no frontend or LLVM dependency.

There are no `compile_project`, `check_project`, `CompilationOptions`,
`CompileOutput`, `DriverError`, or `EmitKind` APIs. Use direct tokenizer/parser
APIs for syntax work; see [parser](frontend/parser.md). Driver APIs support parsing/analysis and the scalar/control-flow executable subset. Public modules are an implementation
surface, not a guarantee of a stable or complete compiler API.
