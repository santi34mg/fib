# Driver and build architecture

The [language specification](../../language-specification.md) is implemented for prelude names, casts, shallow readonly views,
warned pointer field access and exhaustive switches. Concrete generic functions,
multiple source files and fixed-array values execute. Package-rooted lookup, scoped
visibility, installed std sources and Linux x86-64 syscalls execute. Recursive source
discovery remains deferred. Checked runtime indexing/slicing and constant-initialized
globals execute.

`driver.rs` implements `parse_files`, frontend/bootstrap orchestration and
`run_pipeline`. Plans explicitly enumerate files; parsing preserves their IDs and
source context. Empty/missing-entry plans fail before reading files. Analysis uses
`Analyzer::analyze`, including completion validation.

`run_frontend` exposes preparation, parsing and strict analysis without executable
entry validation. `export-symbols --single-file PATH` or `--build-file PATH`
uses it and `symbol_export::public_symbols` to write deterministic versioned JSON
to stdout, with diagnostics on stderr. The inventory includes unrestricted `pub`
top-level source names in the prepared plan (including dependencies), excludes
synthetic namespaces and local declarations, and needs no lowering or LLVM.
See [symbol export](../../symbol-export.md) for the schema and type-display limits.

Both normal and bootstrap projects use `Analyzer::new(...).analyze()` and the single
`ir::lowering::lower` entry point. The bootstrap configures `build_api` as a synthetic
module with ordinary declaration/type identities. The driver validates `main` or
`build` as the selected entry contract and chooses LLVM or the interpreter; lowering
independently revalidates analysis. `driver/entry.rs` validates main's optional
argument slice and environment map parameters and void/int32/Result return.
Extended entries append a generated wrapper to the original parsed entry unit,
reanalyze the units, and select that wrapper through a crate-private entry selector
in the same lowerer. The wrapper exports C `main(argc, argv, envp) int32`; source
main retains internal Fib ABI, including recursive/internal calls. Source std
constructs argument descriptors and the owned byte-string environment map.
Result Ok supplies zero or its int32 payload; Err supplies status 1.
Build entries retain their no-parameter int32 contract. See
[process contracts](../../standard-library.md#process-entry-and-maps).
The normal executable subset includes fixed-width
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
linker names under the versioned [canonical linkage contract](../../roadmap/00-cross-module-linkage.md),
internal linkage and the limited direct Fib fastcc convention
(scalars, structs, enums and pointers); physical ABI
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
`FunctionId -> BuiltinFunction` IR table. Build API text parameters are `const []uint8`;
string literals are readonly byte arrays and implicitly form readonly slices in
that context; explicit `"text"[..]` remains valid. These literal slices become
interpreter-only byte values. General fixed-array-to-slice coercion uses the same
full-slice AST representation as explicit slicing; it does not broaden the narrow
build-file interpreter's supported operations.
Unsupported build-file syntax fails explicitly and this IR is not sent to LLVM.

`BuildPlan::single_file` canonicalizes a source path, derives its namespace from
the UTF-8 stem and chooses an executable beside it without its source extension
(`.exe` on Windows). Sources without a distinct output name fail. The CLI's
`build --single-file PATH` uses this route and prints the resulting binary path.

Build-file interpretation validates IR and executes branches, switches, block
arguments, loops and direct internal calls with separate SSA call frames. Boolean
and fixed-width integer arithmetic, comparisons, numeric casts and immutable byte
values (including byte-slice length) execute. Pointer-width integers, floats,
aggregates, memory operations, indirect/external calls and runtime effects remain
unsupported. The transitive direct-call graph is checked for unsupported instructions
even in untaken blocks. All build operations are staged until execution succeeds;
errors, traps, the 100,000-step budget and the 64-frame call limit leave builder
state unchanged. Each block, instruction and call consumes a step.
Broader `BuilderObject` orchestration remains unfinished. The current finalizer resolves
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
`std::memory::unchecked_deref` intrinsic yields a typed place without that guard. It
emits memory operations rather than a builtin declaration/call and does not select
the bootstrap entry. See [struct/storage semantics](../../struct-storage-semantics.md).
Enum constructors, payload copies and switches lower through the same verified pipeline. See [enum semantics](../../enum-semantics.md).
Concrete generic instances, fixed-array values, multi-bindings and calls across
planned files and lexical defer statements/blocks lower. Constant-initialized globals
and checked runtime array/slice operations also lower. Broader indirect
calls and multiple-result initializers remain unsupported.

`BuildPlan::packages` accepts host-created `PackageSpec` records with `PackageId`,
source root, dependency map and selected file IDs. Validation rejects duplicate
IDs, incomplete/multiple file membership, sources outside roots and unknown
dependency targets. Empty package metadata retains the existing flat-plan adapter.
Entry selection is confined to `plan.entry`; other files' `main` functions are
ordinary internal helpers. Compilation renders the analyzed project's warnings
without changing successful exit status.

## Installed std

The driver prepares plans before parsing, loading an installed version-matched
source tree selected by `--sysroot`, `FIB_SYSROOT`, or the executable installation
prefix. `--no-std` opts out. `BuildPlan::prepare` is idempotent and preserves existing
file IDs and entry selection. Build-file and LSP analysis use the same preparation.
For a standalone document belonging to a std source tree, preparation uses that
tree's manifest and retains the document in the std package instead of creating
an application copy. Only manifest-listed sources are selected.
`std::os` contains Fib wrappers and compiler syscall declarations in one namespace.
Call results may be discarded while retaining their result definitions and effects,
including cleanup calls in defer blocks. See [library contracts](../../standard-library.md).
