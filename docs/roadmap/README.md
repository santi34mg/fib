# Fib roadmap

## Implementation and test baseline (2026-10-19)

This roadmap describes reconstruction and future language work, not completed
compiler features. Source and executable tests take precedence over historical
release notes, samples, and earlier roadmap decisions. See the
[current implementation guide](../agents/README.md) and
[compiler architecture](../compiler-architecture.md).

| Area | Current implementation |
| --- | --- |
| Tokenizer/parser | Working independently, producing the source AST with pending analysis annotations. |
| Supporting utilities | Lexical scope storage, diagnostics, source/file identity records, raw IR access and formatting work. These do not implement language-level analysis. |
| Analysis | Plan/unit ID validation and declaration collection partially work; name traversal is incomplete. Type/member resolution and completion validation remain stubs. No separate typed AST exists. |
| Driver/build | `run_pipeline`, `run_frontend`, `parse_files`, `lower_to_ir` are error stubs. `BuildPlan` names files explicitly; single-file loading, build-file interpretation and builder orchestration are unfinished. |
| SSA IR | Owned arenas, types, instructions, blocks, values and storage records exist. `FunctionBuilder` and `verify_ir` return unimplemented errors. |
| Backend | IR-only phase contracts; no LLVM emission, objects, linking or direct AST fallback. The inactive `backend/lowering/next/` prototype is not a supported route. |
| CLI/execution | Help accepts `build [--single-file PATH]`, `run`, `test`; command execution is `todo!()`. IR interpretation is also unfinished. |
| Tests/corpus | Unit tests exercise parsing, data and stub contracts. `analyzer/test.rs` contains six inline name-resolution specification phases with ordinary failing tests for unfinished semantics; see [validation](../agents/tests/README.md). No integration/e2e harness exists; samples and `std` are not evidence of compilation or execution. |

### Implemented syntax, not implemented semantics

- Bare bit-width types: `int32`, `uint64`, `float32`; `int`/`uint` alias
  32-bit types. Historical byte-width `int4` and `@` type spellings are not
  current source guidance. Some token display helpers/comments still lag.
- Bindings: `x := 1;`, `const x := 1;`, `x: int32;`.
- Functions: `fn f(x: int32) int32 { return x; }`.
- Loops: `while (condition) { ... };`, not legacy `for`.
- Type syntax is still suffix: `int32[8]`, `int32[]`. Prefix forms are proposed.
- Values use `a[i]`, `a[a..b]`, `.len`, and postfix pointer operations
  (`p*`, `x&`), not historical dot-indexing.
- Qualified names retain arbitrary `::` segments. This does not implement
  namespace lookup or import loading; legacy `import` declarations are not
  working grammar.
- Builtin calls retain `@` (for example `@str_len(s)`). Builtin type values
  still parse in expression position. Neither fact implements generic analysis.

See the [parser audit](../agents/src/frontend/parser.md) for details and limits.

## Proposal series

Numbering preserves useful design history; it is not a completion sequence or a
promise that old dependencies still hold. Each page distinguishes the baseline
from proposals. There are no completed semantic or backend milestones here.

| # | Proposal | Prerequisites / reconciliation |
| --- | --- | --- |
| 00 | [Canonical cross-file linkage](00-cross-module-linkage.md) | File/declaration resolution and explicit build-plan membership |
| 01 | [Builtin sigil cleanup](01-builtin-sigil.md) | Type/property spelling already changed; function removal remains a proposal |
| 02 | [Explicit generic functions](02-generic-functions.md) | Decide bracket ambiguity and type-value policy; implement analysis |
| 03 | [Prefix arrays and slices](03-prefix-array-syntax.md) | Decide type-value expression grammar; independent of completed generic support |
| 04 | [Generic data types](04-generic-types.md) | Generic syntax, canonical semantic identity, recursion and layout rules |
| 05 | [String representation redesign](05-remove-string.md) | Literal/FFI/ownership policy; generic owning container for the proposed library model |
| 06 | [Function overloading](06-function-overloading.md) | Canonical declaration identity, complete declaration collection and typing |
| 07 | [Interfaces and constraints](07-interfaces.md) | Generic types/functions, overload rules and conformance identity |

Earlier plans called these decisions “locked.” They remain design inputs, not
implemented guarantees. In particular, removing type values, deleting builtin
functions, return-type overloading and per-instantiation constraint checking
require explicit reconciliation with the current architecture before adoption.

## Reconstruction priorities

### 1. Complete the frontend boundary

- [ ] Load only files explicitly named by `BuildPlan`, preserving file identity
      and source context. Specify build.fib interpretation and single-file plans.
- [ ] Complete declaration binding annotations and lexical/file-name and alias
      resolution, then implement type/member resolution, initialization checking
      and annotation completion on the original AST.
- [ ] Enforce [planned name-resolution rules](../name-resolution.md), including
      redeclarations, visibility, aliases, separate labels and forward visibility
      versus initialization. Storage utilities are not enforcement.
- [ ] Define one conversion/cast matrix; contextual literal ranges; operator,
      assignment, call and return legality; pointer/null and `never` rules.
- [ ] Validate control flow, conditions, reachable returns, enum payload use,
      switch duplicates and definite initialization.
- [ ] Detect alias cycles and infinite by-value types before lowering.
- [ ] Make the six name-resolution specification phases pass, then extend
      positive/negative analysis coverage to types and initialization with source
      locations. Never replace an unfinished pass with successful empty output.

### 2. Build and verify the owned SSA IR

The flat instruction-stream importer and direct typed-AST LLVM route are
historical, not migration targets. There is no fallback to restore.

- [ ] Implement `lower_to_ir(&AnalyzedProject)` with a validated-analysis gate,
      canonical declaration/instance maps and IR-owned runtime types.
- [ ] Implement function-scoped construction: ownership, fresh definitions,
      explicit terminators, block parameters and edge arguments, explicit memory.
- [ ] Implement verification of IDs, ownership, declarations, constants, types,
      dominance, edge arity/types, calls and returns; reject malformed input.
- [ ] Lower short-circuiting and mutable bindings with correct joins and loops;
      model nested lvalues as explicit addresses/projections.
- [ ] Cover aggregates, tuples, arrays, slices, enum payloads, multiple results,
      bounds checks, globals, qualified calls and specified runtime builtins.
- [ ] Preserve diagnostics and source provenance through lowering. Raw records
      and diagnostic dumps are not verified executable IR.

See the [SSA contract](../../src/ir/README.md) for representation details and
open semantic decisions.

### 3. Implement target-aware backend and orchestration

- [ ] Verify input before target/type planning; derive layout from target data,
      including pointer widths, slices, aggregates and aligned enum payloads.
- [ ] Share ABI classification across declarations, definitions, calls and
      returns; specify C vararg promotions, callbacks and unsupported cases.
- [ ] Predeclare all symbols, emit blocks/PHIs, patch forward/back/parallel
      edges using actual emitted predecessors, then verify LLVM before artifacts.
- [ ] Keep language traps, bounds policy and arithmetic semantics explicit; do
      not guess LLVM flags or repair invalid input during emission.
- [ ] Wire driver goals, object production, configured linker arguments, target,
      sysroot, library paths and toolchain diagnostics. No obsolete `--emit`,
      include-root, check or release CLI flags are available today.
- [ ] Implement build-file interpretation, builder operations, IR interpretation
      and CLI execution only with tests proving their real effects.

The [backend plan](../../src/backend/README.md) is an IR-only contract. A future
native backend consumes the same IR; it is not a second frontend lowering route.

## Retained semantic policy proposals

These are intended policies requiring specification and tests, not current
analyzer/runtime behavior:

- Numeric conversions: implicit widening only within a numeric family and
  signedness; explicit narrowing keeps low bits; equal-width signed/unsigned
  casts reinterpret bits; integer/float casts convert numerically.
- Wrapping integer arithmetic; trap on runtime division/remainder by zero and
  diagnose known zero divisors. Specify signed division overflow and shifts.
- Float-to-integer conversion traps for NaN, infinity and out-of-range values;
  integer-to-float conversion may lose precision. Specify NaN/signed-zero and
  optimization rules plus explicit checked-operation APIs.
- Restrict `null` to pointers, with unconstrained type `*void`; restrict `never`
  to return types with no normal return. Neither rule is currently enforced.
- Require commas between list elements, allow one trailing comma and keep
  newlines insignificant. Audit each parser list: current acceptance is not a
  guarantee of consistent strict separator enforcement.
- Lexically scoped `defer`, reverse order on every scope exit, return value
  evaluated first, deferred references evaluated when work starts. Reject
  nested `defer` and `return` within it; decide break/continue behavior.

## Further work retained from earlier phases

### Diagnostics, parser reliability and determinism

- [ ] Harden delimited lists and malformed/truncated input with success and
      negative tokenizer/parser tests; add recovery and no-panic fuzzing.
- [ ] Extend point spans to byte ranges, retaining existing `FileId` identities
      and source context. There is no central source-map store today.
- [ ] Define public diagnostic categories, source chains and machine-readable
      output for the actual driver APIs, not removed compile/check APIs.
- [ ] Use deterministic declaration/instance ordering and repeated-build tests.
- [ ] Add independently callable stages before proposing stage-emission CLI flags.

### Ownership, strings and standard library

- [ ] Specify copy/move, borrowing, mutation, view lifetimes, escaping slices,
      deterministic destruction and suppression, including all control-flow exits.
- [ ] Audit experimental `std` for success-returning no-ops, fake atomics,
      allocation/reallocation failure, overflow, hard-coded pthread layouts and
      signed FFI returns. Old corpus counts and old ABI claims need re-auditing.
- [ ] Classify modules as stub/unsafe/platform-specific/validated; first make
      them parse under current syntax, then add semantic and runtime tests as
      the corresponding compiler stages become executable.
- [ ] Design `Result[T, E]`, `Option[T]`, owned/borrowed bytes, UTF-8 text,
      paths and move-only files; specify allocation failure and embedded NUL.
- [ ] Replace public libc dependencies with a private platform layer, immediate
      errno translation, retry handling and C shims for ABI-sensitive structures.
- [ ] Add robust files/I/O, buffering, formatting, partial-I/O and destruction
      tests before migrating samples and removing the experimental libc facade.
- [ ] Later: process, time, random, networking, threading and genuine atomics.

### Native interoperability

- [ ] Specify function-pointer/callback syntax and ABI; captured closures remain
      outside this proposal.
- [ ] Specify untagged C-union initialization/active-field safety and validate
      size/alignment against C, separately from tagged enums.
- [ ] Add platform-gated X11 bindings/shims and compile/link tests; run display
      tests only where a server is available.

### Tooling, tests and performance

- [ ] Establish an executable integration harness with explicit sample policies
      (output, compile-only, expected error). No `tests/e2e.rs` exists today.
- [ ] Add semantic/backend regressions as those stages work, not old-route
      differential tests or historical probe-file migrations.
- [ ] Separate optimization from bounds policy; add target and CLI policy tests.
- [ ] Measure lowering before optimizing allocation placement or aggregate spills.
- [ ] Audit dependencies and package metadata; keep toolchain/MSRV changes deliberate.
- [ ] Build a Rust compiler-backed LSP after identity/diagnostics stabilize:
      versioned in-memory source provider, cancellation, dependency-aware caches,
      partial-AST recovery, semantic index and UTF-16 position conversions.
- [ ] Test unsaved/transitive files, stale-result suppression, diagnostic clearing,
      Unicode and operation without LLVM; then add hover/navigation/rename and
      semantic tokens rather than relying on heuristic highlighting.
- [ ] Define package discovery and editor configuration around explicit build
      membership, not resurrected recursive import/include-root loading.
- [ ] Later: formatter and package tooling.

## Validation

Use the [validation guide](../agents/tests/README.md). No-default-feature
check/test/help exercise this checkout without LLVM; passing stub tests does not
prove semantic analysis, SSA verification, ABI correctness or execution. Default
`llvm` enables Inkwell 0.8 / LLVM 21 dependencies, not working code generation.
