# Fib roadmap

## Implementation and test baseline (2026-10-24)

This roadmap describes reconstruction and future language work. Source and
executable tests take precedence over historical release notes, samples, and
checklists. See the [current implementation guide](../agents/README.md).

| Area | Current implementation |
| --- | --- |
| Tokenizer/parser | Working source AST construction with pending analysis annotations. |
| Analysis | Names, visibility, aliases, types/members, explicit metatype specialization, bounded type computation, initialization/control flow and completion validation on the original AST. |
| Driver/build | Single-file pipeline and deterministic plan finalization; build files have a narrow interpreted literal-operation API. General interpretation and default discovery remain unfinished. |
| SSA IR | Transactional construction and scalar verification of ownership, signatures, reachability and dominance. Source lowering supports scalar helpers/calls, initialized locals, arithmetic/casts, short-circuit logic, conditionals, while loops, break/continue and returns. Struct/pointer values, stack storage, partial initialization and checked/unchecked dereference are supported. |
| Backend | LLVM 21 emits verified multi-block SSA/PHIs/backedges and scalar operations. C int32 entry and internal direct scalar/struct/pointer Fib fastcc ABI, objects and host linking work; full production ABI and broader representations remain unfinished. |
| CLI/execution | `build --single-file PATH` and narrow `build --build-file PATH` work. `run` and `test` report explicit errors. |
| Tests/corpus | Frontend, driver, IR and backend suites plus LLVM-enabled executable acceptance tests. Most samples have parser coverage; scalar, loop and struct samples additionally have execution coverage. |

## Open proposals

These proposals need design reconciliation or implementation beyond the current
compiler. Their numbering is not an implementation order. Current syntax and
semantics are documented in the [implementation guide](../agents/README.md),
[typing reference](../type-resolution.md), and [scalar semantics](../scalar-semantics.md).

| # | Proposal | Remaining work |
| --- | --- | --- |
| 00 | [Canonical cross-file linkage](00-cross-module-linkage.md) | Cross-file executable lowering, stable symbol encoding and collision policy |
| 02 | [Explicit generic functions](02-generic-functions.md) | Decide bracket syntax and type-value compatibility; lower concrete specializations |
| 03 | [Prefix arrays and slices](03-prefix-array-syntax.md) | Decide type-value expression grammar and implement the chosen syntax |
| 04 | [Generic data types](04-generic-types.md) | Reconcile explicit generic syntax with metatype specialization; implement runtime layout/lowering |
| 05 | [String representation redesign](05-remove-string.md) | Literal/FFI/ownership policy and ordinary standard-library string APIs |
| 06 | [Function overloading](06-function-overloading.md) | Overload selection, return-type policy and canonical overload identity |
| 07 | [Interfaces and constraints](07-interfaces.md) | Constraint syntax, conformance identity and specialization checking |
| 08 | [Interpreted build files](08-interpreted-build-files.md) | Default discovery, general IR interpretation and broader builder orchestration |

## Remaining implementation work

### Runtime lowering and SSA

- [ ] Lower tuples, arrays, slices, enum payloads, multiple results,
      globals and dynamic bounds checks.
- [ ] Lower concrete generic specializations and calls across explicitly planned
      source files, preserving canonical declaration/instance identities.
- [ ] Lower switch and lexical defer, including cleanup on return, break and
      continue, according to [implemented flow semantics](../type-resolution.md).

See the [SSA contract](../../src/ir/README.md) for representation boundaries.

### Backend and orchestration

- [ ] Extend target-derived layout to slices, tuples, arrays and enums.
- [ ] Extend shared ABI classification beyond the internal direct scalar/struct/pointer Fib subset;
      implement external scalar C interoperability and broader representations.
- [ ] Specify and implement C vararg promotions, callbacks and indirect calls.
- [ ] Add configurable linker arguments, targets, sysroots, library paths and
      toolchain diagnostics beyond the current host-linking path.
- [ ] Implement default build-file discovery, general CFG/value/function
      interpretation and broader build API orchestration through the shared IR
      path. Preserve centralized plan validation and the opaque host-owned API.
- [ ] Implement the `run` and `test` commands with explicit execution contracts.

See the [backend contract](../../src/backend/README.md) and
[interpreted build-file plan](08-interpreted-build-files.md).

### Diagnostics, parser reliability and determinism

- [ ] Harden delimited lists and malformed/truncated input with success and
      negative tokenizer/parser tests; audit comma/trailing-comma consistency,
      add recovery and no-panic fuzzing.
- [ ] Extend point spans to byte ranges, retaining existing `FileId` identities
      and source context. There is no central source-map store today.
- [ ] Define public diagnostic categories, source chains and machine-readable
      output for the actual driver APIs, not removed compile/check APIs.
- [ ] Extend determinism coverage to cross-file symbols, generic instances and
      complete repeated-build artifacts.

### Ownership, strings and standard library

- [ ] Specify copy/move, borrowing, mutation, view lifetimes, escaping slices,
      deterministic destruction and suppression, including all control-flow exits.
- [ ] Audit experimental `std` for success-returning no-ops, fake atomics,
      allocation/reallocation failure, overflow, hard-coded pthread layouts and
      signed FFI returns.
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

- [ ] Extend the existing executable harness to broader sample policies:
      expected output, compile-only cases and expected diagnostics.
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
      membership.
- [ ] Later: formatter and package tooling.

## Validation

Use the [validation guide](../agents/tests/README.md). Run frontend/IR checks
without default features and LLVM-enabled checks for backend changes. Executable
acceptance tests require LLVM 21 and host `cc`. Add positive and negative coverage
for each newly supported behavior and update the implementation guide alongside it.
