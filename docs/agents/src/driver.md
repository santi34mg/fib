# Driver and build architecture

[`driver.rs`](../../../src/driver.rs) defines `PipelineGoal::{Check, Ir, Objects,
Binary}`, `SourceUnit { file, ast }`, and these explicit error-returning stubs:

- `run_pipeline`: orchestration;
- `run_frontend`: project frontend;
- `parse_files`: read/tokenize/parse plan files;
- `lower_to_ir`: sole intended `AnalyzedProject -> Ir` boundary.

No filesystem discovery, import resolution, analysis, emission, or linking runs
through these entry points. Tests in `src/driver/test.rs` assert explicit failure,
including for empty input, not successful compilation.

[`build_plan.rs`](../../../src/build_plan.rs) owns `FileId`, `FileName`,
`SourceFile { id, name, path }`, and `BuildPlan { files, entry, output }`.
`BuildPlan::single_file` is `todo!()`. Named plan files are intended to be the
complete namespace universe; semantic path resolution must not discover files.

[`builder_object.rs`](../../../src/builder_object.rs) has builder state and an
`add_build_file` directory scan. That scan compares paths directly to `build.fib`;
do not assume reliable build-file discovery. Compilation/interpretation of the
build file, plan creation, build, and run are `todo!()`, as is
[`interpret_ir`](../../../src/ir_interpreter.rs).

`driver/compiler_output.rs` defines `CompilationOutput::{Tokens, Ast, Checked,
Ir, Objects, Binary}`. These are output shapes, not implemented emit modes.
Old comments referring to `compile()` or CLI printing are not evidence of code.

Future lowering must validate annotation completeness and build owned SSA IR;
there must be no direct AST-to-backend fallback. Old include-root ordering,
canonical import deduplication, clang selection, output policy, and runtime
bounds behavior are not implemented current contracts.
