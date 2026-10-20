# Driver and build architecture

`driver.rs` implements `parse_files`, `run_frontend`, `lower_to_ir` and
`run_pipeline`. Plans explicitly enumerate files; parsing preserves their IDs and
source context. Empty/missing-entry plans fail before reading files. Analysis uses
`Analyzer::analyze`, including completion validation.

`lower_to_ir` independently revalidates analysis and delegates to `driver/lowering`.
The executable subset is one file with `main() int32` and int32 helper definitions,
direct calls, initialized int32/bool locals (including const), assignments,
comparisons, nested if/else and returns. Negative int32 literals are supported.
All source forms are prevalidated, including dead branches. Unsupported constructs
produce Backend diagnostics with spans and original plan paths retained by
`AnalyzedProject::source_paths`.

Lowering predeclares function identities and constants, then transactionally
constructs verified SSA. Locals map resolved identities to immutable operands;
assignments replace mappings without allocating storage. Conditional joins merge
changed outer bindings through block parameters. Returning arms do not contribute
to joins; branch-local bindings do not escape. Known bool conditions use the same
evaluator as frontend flow checking. Helpers have deterministic identity-based
linker names, internal linkage and centrally selected C convention; physical ABI
argument placement belongs exclusively to the backend.

`PipelineGoal::{Check, Ir, Objects, Binary}` stops after analysis, verified IR,
LLVM object emission, or linking respectively. Objects use the output path with
`.o`; binaries use `BuildPlan.output`. Output is staged beside the destination,
then renamed only after success. Source-overwriting paths are rejected. Binary
linking invokes host `cc`, preserving stderr on failure. Object output needs LLVM
21; binary output additionally needs a compatible C linker/runtime.

`BuildPlan::single_file` canonicalizes a source path, derives its namespace from
the UTF-8 stem and chooses an executable beside it without its source extension
(`.exe` on Windows). Sources without a distinct output name fail. The CLI's
`build --single-file PATH` uses this route and prints the resulting binary path.

Build-file interpretation, `BuilderObject` orchestration and `interpret_ir` remain
unfinished. `run`, `test` and build without `--single-file` return explicit errors.
Arithmetic, source loops/defer, uninitialized declarations, globals/storage,
aggregates, indirect calls and concrete generic bodies remain unsupported.
