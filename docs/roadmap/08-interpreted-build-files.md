# 08 — Interpreted build files

## Status

This document defines a future implementation plan. Build-file discovery,
build-file lowering, IR interpretation, the build API and project-plan construction
are not implemented in the current compiler. `fib build --single-file PATH` remains
the only working build entry while this plan is unfinished.

The chosen direction is to compile `build.fib` through the normal frontend into
verified, backend-independent IR and interpret that IR inside `fibc`. The compiler
will not emit, load or execute a native build-file binary.

## Goals

- Make `build.fib` the explicit source of a project's `BuildPlan`.
- Reuse the normal parser, analyzer, lowering boundary and owned IR verifier.
- Keep build evaluation independent of LLVM, a native linker and host executable
  loading.
- Expose a versioned Fib build API without exposing Rust layouts or Rust ABI.
- Produce an immutable plan that is validated before any project source is read.
- Preserve source-aware diagnostics across build-file compilation, interpretation
  and plan validation.

## Discovery and project root

`fib build` will look for exactly `build.fib` in the process working directory. The
initial implementation will not walk parent directories. A future explicit
`--build-file PATH` option may select another file without changing the default.

The directory containing the selected build file is the project root. Relative
source, dependency and output paths supplied by the build file are resolved against
that root, not against a mutable interpreter working directory. Discovery and path
normalization must diagnose missing files, non-files, invalid UTF-8 requirements
where applicable and outputs that would overwrite source inputs.

`fib build --single-file PATH` remains a separate mode and does not discover or
execute `build.fib`.

## Bootstrap compilation

The project plan does not exist until `build.fib` runs, so the driver must create a
small internal bootstrap plan containing:

- the selected `build.fib` source;
- the compiler-provided build API declarations or namespace;
- the distinguished build entry contract; and
- the source identity and diagnostic context needed by the normal frontend.

This bootstrap plan is not the resulting project `BuildPlan`, and `build.fib` is not
a project source merely because it was evaluated. Initially, build files should be
limited to the compiler-provided API and language facilities supported by the IR
interpreter. Allowing third-party dependencies needed to compile the build file
would introduce a bootstrap cycle and requires a separate future design.

The driver will parse, analyze and lower the bootstrap project through the same
validated boundaries used by ordinary source. The owned IR must pass `verify_ir`
before interpretation. Unsupported source or IR operations must fail explicitly;
they must never be treated as successful no-ops.

## Build entry and API boundary

A build file will provide exactly one distinguished `build` entry with a
compiler-defined signature. Its final source syntax remains to be specified after
opaque handles, paths, strings and build errors have stable semantic
representations. Conceptually, it receives a host-owned plan builder and mutates it:

```fib
// Illustrative future API; this is not guaranteed parser syntax.
pub fn build(builder: *PlanBuilder) void {
    builder.add_source("src/main.fib");
    builder.set_entry("main");
    builder.set_output("build/app");
}
```

The boundary is a language-level interpreter API, not native FFI:

- No Rust value such as `PathBuf`, `Vec` or the current `BuilderObject` crosses the
  boundary.
- No `callconv "rust"` contract is introduced; Rust ABI and layouts are not stable
  public interfaces and must not couple Fib projects to the current compiler
  implementation language.
- A native C trampoline is unnecessary for interpreted execution. Calling
  convention, symbol export and bodyless external declarations remain separate
  language concepts.
- The interpreter receives an opaque handle to host-owned builder state and
  dispatches only supported, versioned build operations.
- The build function does not return a Rust-shaped builder by value. The host
  finalizes its own builder after successful interpretation.

The public API should prefer typed operations over arbitrary compiler or linker
flag strings. Raw arguments may be considered later as explicit backend-specific
escape hatches, not as the primary representation of optimization, target or
linking policy.

## Plan data and dependency categories

The mutable API object and immutable output have separate responsibilities:

- `PlanBuilder` is interpreter-facing mutable state owned by the driver.
- `BuildPlan` is the finalized, validated data consumed by parsing, analysis and
  later pipeline stages.
- Driver/orchestration state is not part of either public build-file value.

The compiler assigns `FileId` values during finalization. Build scripts name paths
and logical source names; they do not manufacture compiler identities.

The API must not overload one undifferentiated `add_dependency` operation. Its data
model should distinguish at least:

- project source files and their logical names;
- Fib package dependencies;
- native link libraries and search paths;
- target and profile settings; and
- declared build inputs such as files or environment variables.

Build-script dependencies, if supported later, are a separate bootstrap concern.
Package resolution itself is outside this plan and must not reintroduce implicit
recursive source discovery.

## Interpretation and capabilities

`interpret_ir` will execute verified IR directly. It must not invoke the LLVM
backend, link a host executable, use a JIT as a fallback, load a shared object or
implement an in-memory ELF/PE/Mach-O loader. This avoids a native ABI boundary and
prevents malformed generated machine code from sharing the compiler's address
space.

Interpretation alone is not a complete security sandbox. The initial interpreter
should expose only the operations required by the versioned build API. Filesystem,
environment, process and network access require explicit capability and
reproducibility policies before being made available.

Host and project target are separate values. A build file is interpreted as
backend-independent IR and may query explicit host or target information through
the build context when the API defines those operations. It is never compiled as a
native artifact for the project's target.

## Finalization and validation

After `build` returns successfully, the host finalizes the builder and validates the
complete plan before compiling project sources. Validation must include:

- exactly one entry source that belongs to the explicit file set;
- unique file IDs, logical names and normalized memberships;
- deterministic ordering and compiler-assigned identities;
- existence and kind checks for all required source paths;
- project-root-relative path resolution with a documented policy for absolute paths
  and `..` components;
- output paths that do not overwrite `build.fib` or project sources;
- valid, non-contradictory target, profile and linker settings; and
- no mutation after finalization.

Build-file parse, analysis and lowering failures use normal compiler diagnostics.
Interpreter traps and build-API failures should retain source locations whenever IR
provenance permits. Plan-finalization errors must identify the offending build
operation or path. No partial `BuildPlan` proceeds into the project pipeline.

## Reproducibility and caching

Caching is not required for the first implementation, but the API must leave room
to record observable inputs. Future cache keys should include at least:

- `build.fib` and permitted build-file dependency contents;
- compiler and build-API versions;
- interpreter semantics version;
- explicit host and project target configuration; and
- declared file and environment inputs.

Undeclared ambient reads make correct invalidation impossible. A capability-based
interpreter API should eventually record allowed observations as they occur.

## Implementation sequence

1. Replace ad-hoc build-file discovery with exact `<cwd>/build.fib` discovery and
   project-root path anchoring, while preserving the current explicit error until
   interpretation is available.
2. Define and validate the bootstrap plan and distinguished build entry without
   adding a second frontend or direct AST execution path.
3. Define opaque interpreter values and a minimal versioned `PlanBuilder` API. Keep
   Rust types and native calling conventions out of the contract.
4. Implement the required IR interpreter operations with source-aware traps and
   strict rejection of unsupported IR.
5. Finalize and centrally validate `BuildPlan`, including deterministic file-ID
   allocation and path/output safety.
6. Connect `fib build` to bootstrap lowering, verified interpretation and the
   existing project pipeline. Preserve `--single-file` as a bypass.
7. Add declared inputs, caching and broader capabilities only after their
   invalidation and security semantics are specified.

Each step must update the current implementation guide when it becomes real. The
source and tests remain authoritative over this plan.

## Acceptance criteria

The first complete milestone requires tests proving that:

- `fib build` finds only `<cwd>/build.fib` and reports a missing build file;
- relative paths are anchored to the build-file directory;
- malformed or semantically invalid build files produce normal diagnostics;
- invalid IR is rejected before interpretation;
- a valid build file creates a deterministic multi-file `BuildPlan`;
- duplicate names, missing entry files and source-overwriting outputs are rejected;
- unsupported interpreter operations fail rather than succeeding silently;
- plan construction works without the LLVM feature or a host linker;
- `--single-file` does not execute a neighboring build file; and
- interpretation never emits, loads or executes a native build-file artifact.

## Remaining design questions

The architectural route is decided, but these details remain open:

- the exact source signature and namespace/import mechanism for the build API;
- semantic representations for opaque handles, paths, strings and recoverable build
  errors;
- the minimum IR operation set required by useful build files;
- package-resolution and future build-dependency models;
- the initial capability policy for filesystem and environment observations; and
- the serialized or in-memory form used for cache entries.
