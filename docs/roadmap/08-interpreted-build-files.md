# 08 — Interpreted build files

## Status

This document defines the remaining implementation plan. The compiler now has an
initial synthetic `build_api` namespace, shared analysis, verified CFG lowering
and identity-based interpreter dispatch into
`PlanBuilder`. Initial deterministic project-plan finalization validates root-relative
sources, names, entry and output safety. CFG/block arguments, loops, direct internal
calls, boolean/fixed-width integer operations and immutable byte values execute with
step/depth limits and transactional builder effects. Memory, floats, indirect calls
and broader builder orchestration remain unsupported. Both `fib build --single-file PATH`
and the narrow `fib build --build-file PATH` route compile supported programs.
Explicit multi-file plans now lower and execute. Package metadata, scoped visibility
and package-rooted lookup work. Recursive discovery remains deferred. Build-file
analysis loads installed std through shared plan preparation, while interpretation
rejects runtime syscalls.

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
discovery implementation will not walk parent directories. The existing explicit
`--build-file PATH` option selects a build file today; bare `fib build` still
returns an error.

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

The current build file provides `build() int32` and calls the synthetic
`build_api` namespace with explicitly sliced string literals. Its text parameters
have type `const []uint8`; literals are readonly `uint8` arrays, and array-to-slice
conversion is explicit:

```fib
fn build() int32 {
    build_api::add_source("src/main.fib"[..]);
    build_api::set_entry("main"[..]);
    build_api::set_output("build/app"[..]);
    return 0;
}
```

The broader API's source signature remains open until opaque handles, paths,
strings and recoverable build errors have stable representations. Conceptually,
a future API could receive a host-owned plan builder and mutate it:

```fib
// Illustrative future API; this is not guaranteed parser syntax.
pub fn build(builder: *PlanBuilder) void {
    builder.add_source("src/main.fib"[..]);
    builder.set_entry("main"[..]);
    builder.set_output("build/app"[..]);
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
Package acquisition/version resolution remains outside this plan. Recursive source
selection is explicitly enabled through a build-API operation; name resolution
never recursively loads files on demand. See the confirmed source-discovery rules
below.

## Confirmed package source discovery

Each package has an explicit source root, separate from the directory containing
`build.fib` used to anchor build operations. A recursive discovery operation
(exact API name/signature pending) selects `.fib` files beneath a requested directory
inside that root. Namespace paths remain relative to the package source root,
not the discovery subtree. It skips symlinks and the active build file, supports
explicit exclusions, and otherwise includes hidden/test/build directories without
conventional-name filtering. Repeated selection of one file includes it once.

Finalize the selected source tree deterministically before analysis. Normalize
hyphens to underscores and compare package/directory/file names without case;
reject spaces, language keywords, normalized-name collisions and file/directory
conflicts. Declaration/local names remain case-sensitive. The confirmed spelling
`root` means the package containing the source, including in dependencies; package
names cannot be shadowed, and explicit namespace aliases can shorten paths.

`BuildPlan::packages` represents package IDs, source roots, dependency maps and
file membership for host-created plans. The build API does not expose that metadata
yet; current finalization still uses file-stem names and rejects duplicate source selections. It must be extended rather than claiming discovery already
works. Unicode folding and host-supplied persistent acquisition/version identities
are implemented by the [canonical linkage contract](00-cross-module-linkage.md);
the build API still needs to expose package registration and acquisition.
See [language specification](../language-specification.md) for scoped visibility and exports.

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
- unique file IDs and normalized namespace paths, with repeated selection of
  the same source deduplicated under the confirmed discovery policy;
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

1. Extend plan data and finalization with package source roots and the confirmed
   recursive source-discovery/normalization rules, retaining project-root anchoring
   for build operations. Add exact `<cwd>/build.fib` discovery using the existing
   explicit build-file bootstrap path.
2. Extend the bounded CFG/value interpreter to broader
   CFG/value/function interpretation, with source-aware traps and explicit errors
   for unsupported IR. Keep Rust layouts and native ABI out of the API.
3. Extend centralized finalization as target/profile/linker operations are added;
   preserve existing deterministic source IDs and path/name/entry/output checks.
4. Support executable cross-file lowering before promising multi-file builds;
   plan finalization and analysis already support explicit multi-file membership.
5. Add declared inputs, caching and broader capabilities only after their
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
- overlapping source selections include each file once; normalized namespace
  collisions, missing entries and source-overwriting outputs are rejected;
- recursive discovery preserves source-root paths, applies explicit exclusions,
  skips symlinks and the active build file, and has deterministic results;
- unsupported interpreter operations fail rather than succeeding silently;
- plan construction works without the LLVM feature or a host linker;
- `--single-file` does not execute a neighboring build file; and
- interpretation never emits, loads or executes a native build-file artifact.

## Remaining design questions

The architectural route is decided, but these details remain open:

- the broader API signature and namespace evolution beyond `build() int32`
  and the existing synthetic `build_api` namespace;
- semantic representations for opaque handles, paths, strings and recoverable build
  errors;
- the minimum IR operation set required by useful build files;
- package-resolution and future build-dependency models;
- the initial capability policy for filesystem and environment observations; and
- the serialized or in-memory form used for cache entries.
