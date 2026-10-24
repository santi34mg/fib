# Driver and build architecture

`driver.rs` implements `parse_files`, frontend/bootstrap orchestration and
`run_pipeline`. Plans explicitly enumerate files; parsing preserves their IDs and
source context. Empty/missing-entry plans fail before reading files. Analysis uses
`Analyzer::analyze`, including completion validation.

Both normal and bootstrap projects use `Analyzer::new(...).analyze()` and the single
`ir::lowering::lower` entry point. The bootstrap configures `build_api` as a synthetic
module with ordinary declaration/type identities. The driver validates `main` or
`build` as the selected entry contract and chooses LLVM or the interpreter; lowering
independently revalidates analysis. The normal executable subset is one file with `main() int32`, fixed-width
integer/float/bool helper definitions and calls, initialized locals (including
const), structs/typed pointers, partial initialization, stack storage, field and
pointer mutation, assignments, scalar arithmetic/bitwise/shifts, explicit numeric casts,
short-circuit booleans, comparisons, nested if/else, while loops, break/continue and returns.
All source forms are prevalidated, including dead branches. Unsupported constructs
produce Backend diagnostics with spans and original plan paths retained by
`AnalyzedProject::source_paths`.

Lowering predeclares function identities and constants, then transactionally
constructs verified SSA. Initialized scalars without address-taking map resolved
identities to immutable operands. Structs, uninitialized locals and address-taken
bindings use entry-allocated stack slots with initialization at the source
declaration; assignments and alias accesses use explicit loads/stores. Conditional joins merge
changed outer bindings through block parameters. Returning arms do not contribute
to joins; branch-local bindings do not escape. Loops carry SSA bindings visible at
entry through ordered header/exit block parameters. Continues return current values
to the header; breaks merge current values at the innermost loop exit. Dynamic
conditions are reevaluated on every iteration, including short-circuit conditions.
Absent/known-true conditions have no condition-false exit; exit blocks are created
only when reachable. Known-false bodies are prevalidated without emitting blocks.
Known bool conditions use the same evaluator as frontend flow checking. Helpers have deterministic identity-based
linker names, internal linkage and the limited direct Fib fastcc convention
(scalars, structs and pointers); physical ABI
argument placement belongs exclusively to the backend.

`PipelineGoal::{Check, Ir, Objects, Binary}` stops after analysis, verified IR,
LLVM object emission, or linking respectively. Objects use the output path with
`.o`; binaries use `BuildPlan.output`. Output is staged beside the destination,
then renamed only after success. Source-overwriting paths are rejected. Binary
linking invokes host `cc`, preserving stderr on failure. Object output needs LLVM
21; binary output additionally needs a compatible C linker/runtime.

`compile_build_file_to_ir` is bootstrap orchestration, not a separate frontend or
lowering route. It configures the compiler-provided synthetic
`build_api::{add_source, set_entry, set_output}` module on the ordinary analyzer, validates
one `build() int32` entry, and invokes the same lowering used by normal source. Builtins retain semantic `DeclId` identities through a
`FunctionId -> BuiltinFunction` IR table; strings become interpreter-only byte values.
Unsupported build-file syntax fails explicitly and this IR is not sent to LLVM.

`BuildPlan::single_file` canonicalizes a source path, derives its namespace from
the UTF-8 stem and chooses an executable beside it without its source extension
(`.exe` on Windows). Sources without a distinct output name fail. The CLI's
`build --single-file PATH` uses this route and prints the resulting binary path.

Build-file interpretation is only an initial straight-line subset: `interpret_ir`
validates IR and dispatches the three build operations by builtin identity into
host-owned `PlanBuilder` state. Multi-block control flow is rejected before
applying build operations. General CFG/value/function interpretation and
`BuilderObject` orchestration remain unfinished. The current finalizer resolves
source/output paths against the build-file directory, assigns deterministic IDs and
stem-derived logical names, validates the selected entry, and rejects duplicate,
missing, escaping or overwriting paths. The path remains fixed as `<cwd>/build.fib` discovery, bootstrap
compilation to verified owned IR, interpretation through an opaque host-owned build
API, and centralized `BuildPlan` finalization. It will not emit or load a native
build-file executable and will not expose Rust ABI or Rust object layouts. See the
[interpreted build-file plan](../../roadmap/08-interpreted-build-files.md). `run`,
`test` remains unimplemented; `build --build-file PATH` supports the narrow interpreted
subset and then enters the normal binary pipeline.
Ordinary dereference emits a null-check/trap CFG. The automatically registered
`memory::unchecked_deref` intrinsic yields a typed place without that guard. It
emits memory operations rather than a builtin declaration/call and does not select
the bootstrap entry. See [struct/storage semantics](../../struct-storage-semantics.md).
Defer, runtime globals, arrays/slices/enums, indirect calls and concrete generic
bodies remain unsupported.
