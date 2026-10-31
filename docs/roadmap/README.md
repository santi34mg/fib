# Fib roadmap

## Implementation and test baseline (2026-10-31)

This roadmap describes reconstruction and future language work. Source and
executable tests take precedence over historical release notes, samples, and
checklists. See the [current implementation guide](../agents/README.md).
The [language specification](../language-specification.md) defines the
package/prelude, prefix-array, numeric, mutability and exhaustive-switch design;
several rules require implementation changes beyond this baseline.

| Area | Current implementation |
| --- | --- |
| Tokenizer/parser | Working source AST construction with pending analysis annotations. |
| Analysis | Names, visibility, aliases, types/members, explicit metatype specialization, bounded type computation, initialization/control flow and completion validation on the original AST. |
| Driver/build | Multi-file pipeline, explicit package metadata and deterministic plan finalization; build files have a narrow interpreted literal-operation API. General interpretation and default discovery remain unfinished. |
| SSA IR | Transactional construction and scalar/aggregate verification of ownership, signatures, reachability and dominance. Source lowering supports scalar helpers/calls, initialized locals, arithmetic/casts, short-circuit logic, conditionals, while loops, break/continue and returns. Struct/pointer values, stack storage, partial initialization and checked/unchecked dereference are supported. Enums support constructors, payload copies, switches, storage and internal calls. |
| Backend | LLVM 21 emits verified multi-block SSA, arrays/slices, globals and direct/indirect calls. Linux x86-64 System V C scalar/pointer/nonempty-struct boundaries, internal Fib fastcc, objects and native host linking work; broader ABI types/targets remain unfinished. |
| CLI/execution | `build --single-file PATH` and narrow `build --build-file PATH` work. `run` and `test` report explicit errors. |
| Tests/corpus | Frontend, driver, IR and backend suites plus LLVM-enabled executable acceptance tests. All top-level samples have parser and compile/run coverage with output/exit checks; installed std has dedicated executable coverage. |

## Active design and implementation work

These documents track remaining design or implementation beyond the current
compiler. Their numbering is not an implementation order. Current syntax and
semantics are documented in the [implementation guide](../agents/README.md),
[typing reference](../type-resolution.md), and [scalar semantics](../scalar-semantics.md).

| # | Proposal | Remaining work |
| --- | --- | --- |
| 00 | [Package paths and canonical linkage](00-cross-module-linkage.md) | Complete: persistent package identities, Unicode normalization and versioned canonical symbols |
| 05 | [String representation redesign](05-remove-string.md) | Literal/FFI/ownership policy and ordinary standard-library string APIs |
| 06 | [Function overloading](06-function-overloading.md) | Overload selection, return-type policy and canonical overload identity |
| 07 | [Interfaces and constraints](07-interfaces.md) | Constraint syntax, conformance identity and specialization checking |
| 08 | [Interpreted build files](08-interpreted-build-files.md) | Package/discovery build APIs, recursive source discovery, default build-file discovery and general interpretation |

## Language specification

The [specification](../language-specification.md) defines package-rooted file
namespaces, scoped visibility, implicit-prelude type identifiers, metatype
specialization, prefix arrays/slices, numeric behavior, mutability and exhaustive
switches. Semantic specialization, caching and concrete body rechecking work;
concrete rechecked instances also execute through verified IR and LLVM.

## Remaining implementation work

### Frontend and selected language changes

- [x] Resolve all type names as ordinary identifiers through a protected implicit
      prelude; preserve unified bindings and metatype specialization.
- [x] Implement package/source-root identities, `root` paths and namespace aliases;
      normalize Unicode case/hyphens for package/directory/file segments, reject collisions,
      and retain case-sensitive declaration/local names. Canonical linkage uses persistent package identities.
- [x] Add `pub(dir)` and `pub(root)`, preserving file-private/default and public
      access; permit explicit re-exports of accessible restricted targets.
- [x] Give direct same-family literal cast operands destination context; implement
      explicit numeric/bool conversions with zero/nonzero and 0/1 semantics.
- [x] Permit shallow writable-to-readonly pointer/slice views and warned one-layer
      implicit pointer field dereference, preserving null checks and permissions.
- [x] Require exhaustive enum switches, allowing `when else` for remaining variants.

### Runtime lowering and SSA

- [x] Lower checked runtime indexing/slicing and constant-initialized globals.
- [ ] Lower general tuples and multiple-result source functions.
- [x] Lower concrete generic specializations and calls across explicitly planned
      source files, preserving canonical declaration/instance identities.
- [x] Lower lexical defer, including cleanup on return, break and
      continue, according to [implemented flow semantics](../type-resolution.md).

See the [SSA contract](../../crates/fib-ir/src/ir/README.md) for representation boundaries.

### Backend and orchestration

- [ ] Extend target-derived layout to general tuples; arrays and slices execute.
- [ ] Extend shared ABI classification beyond the internal direct scalar/struct/enum/pointer Fib subset;
      implement broader representations and C types/targets beyond the supported
      Linux x86-64 System V scalar/pointer/nonempty-struct subset.
- [x] Implement function addresses, callbacks and indirect calls with per-signature ABI.
- C variadics are removed; fixed-signature C shims are required.
- [x] Support native C sources, library names/search paths and installed std sysroots.
- [ ] Add general linker arguments, cross-target CLI configuration and broader
      toolchain diagnostics beyond the current host-linking path.
- [ ] Add build-API recursive source discovery under explicit package source roots:
      select `.fib` files, skip symlinks, exclude the active build file, support
      explicit exclusions and deduplicate selections into a deterministic plan.
- [ ] Implement default build-file discovery, general CFG/value/function
      interpretation and broader build API orchestration through shared IR.
      Preserve centralized validation and the opaque host-owned API.
- [ ] Implement the `run` and `test` commands with explicit execution contracts.

See the [backend contract](../../crates/fib-backend/src/backend/README.md) and
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
- [ ] Extend installed source std beyond the implemented memory, formatting, I/O,
      strings, process, maps, results, randomness and OS modules. Validate
      allocation failure, overflow, genuine atomics and platform FFI contracts
      rather than restoring unchecked bindings or success-returning stubs.
- [x] Provide installed, versioned std/prelude registration, reusable `Result(T, E)`
      and Linux x86-64 typed syscall wrappers through verified IR and LLVM.
- [ ] Design option types, owned/borrowed bytes, UTF-8 text,
      paths and move-only files; specify allocation failure and embedded NUL.
- [ ] Replace public libc dependencies with a private platform layer, immediate
      errno translation, retry handling and C shims for ABI-sensitive structures.
- [x] Implement shared formatting and output retry loops, and migrate top-level
      samples to supported source std APIs.
- [ ] Add owned files, buffering, reader behavior and destruction contracts/tests.
- [x] Implement process arguments/environment entry adaptation and OS randomness.
- [ ] Later: time, networking, threading and genuine atomics.

### Native interoperability

- [x] Implement function-pointer/callback syntax and ABI, including struct fields.
      Captured closures remain outside this implementation.
- [ ] Specify untagged C-union initialization/active-field safety and validate
      size/alignment against C, separately from tagged enums.
- [ ] Add platform-gated X11 bindings/shims and compile/link tests; run display
      tests only where a server is available.

### Tooling, tests and performance

- [x] Discover and execute all top-level samples with expected output/exit checks.
- [ ] Extend sample policies to compile-only cases and expected diagnostics.
- [ ] Separate optimization from bounds policy; add target and CLI policy tests.
- [ ] Measure lowering before optimizing allocation placement or aggregate spills.
- [ ] Audit dependencies and package metadata; keep toolchain/MSRV changes deliberate.
- [x] Build a Rust compiler-backed LSP with unsaved overlays, collecting diagnostics,
      semantic tokens, UTF-16 positions and qualified-name completion.
- [x] Test unsaved dependencies, stale-result suppression, diagnostic clearing,
      Unicode and operation without LLVM.
- [ ] Extend LSP recovery/indexing and add local/field completion, hover, navigation
      and rename.
- [ ] Implement editor configuration around finalized package/source membership;
      specify package acquisition and version identity separately from source discovery.
- [ ] Later: formatter and package tooling.

## Validation

Use the [validation guide](../agents/tests/README.md). Run frontend/IR checks
without default features and LLVM-enabled checks for backend changes. Executable
acceptance tests require LLVM 21 and host `cc`. Add positive and negative coverage
for each newly supported behavior and update the implementation guide alongside it.
