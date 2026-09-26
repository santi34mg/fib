# Fib Roadmap

This document tracks outstanding language, compiler, tooling, and standard
library work. Completed work belongs in `CHANGELOG.md` and is removed from this
roadmap.

## Ordering Constraints

1. Restore semantic and lowering soundness.
2. Add canonical package, module, and declaration identity.
3. Add target-aware ABI and layout support.
4. Add generic data types.
5. Add ownership, borrowing, and deterministic destruction.
6. Add interfaces and static dispatch.
7. Replace the experimental libc-based standard library.
8. Add higher-level tooling after language semantics stabilize.

## Locked Language Decisions

These decisions constrain implementation and are not open roadmap questions.

### Numeric Behavior

- Allow implicit conversions only to a wider type of the same numeric family
  and signedness.
- Reject implicit signed/unsigned and integer/floating-point conversions.
- Make explicit integer narrowing keep the low-order bits.
- Make explicit equal-width signed/unsigned casts reinterpret the bit pattern.
- Make explicit integer/floating-point casts perform numeric conversion rather
  than bit reinterpretation.
- Make integer arithmetic wrap, including overflow during negation and signed
  division.
- Trap on runtime division or remainder by zero.
- Reject division or remainder by zero when known at compile time.
- Trap when converting NaN, infinity, or an out-of-range float to an integer.
- Permit lossy integer-to-float conversion without trapping.
- Provide explicit checked builtins for operations whose default can trap.

### `null` and `@never`

- Restrict `null` assignment and comparison to pointer types.
- Type unconstrained `null` as `*@void`.
- Restrict `@never` to function return types.
- Require an `@never` function to have no reachable return.

### Separators

- Require commas between list elements and allow one trailing comma.
- Require semicolons after statements.
- Keep newlines semantically insignificant.

### `defer`

- Give each deferred statement to its innermost lexical block.
- Run deferred statements in reverse declaration order.
- Run deferred statements when their scope exits through fallthrough, return,
  break, or continue.
- Evaluate a return expression before running deferred statements.
- Resolve values used by deferred work when that work starts.
- Reject `defer` inside `defer` and `return` inside `defer`.
- Permit `if`, `while`, and `switch` inside `defer`.

## Unresolved Language Decisions

Resolve each item in maintained language documentation and tests before
implementing work that depends on it.

- [ ] Define integer-literal contextual typing and range behavior.
- [ ] Define shift-count behavior and division overflow edge cases.
- [ ] Define the names and contracts of checked arithmetic and conversion
      builtins.
- [ ] Define string ownership, lifetime, mutability, and representation.
- [ ] Define embedded-NUL and FFI string ownership behavior.
- [ ] Define how standard library APIs report allocation failure.
- [ ] Define module visibility, re-export, ambiguity, and collision policies.
- [ ] Define duplicate declaration and foreign-symbol policies.
- [ ] Define nested-scope shadowing rules.
- [ ] Define slice ownership, aliasing, mutation, and escape rules.
- [ ] Define NaN, signed-zero, infinity, and optimization behavior.
- [ ] Define untagged C-union syntax and safety.
- [ ] Define function-value syntax and semantics.
- [ ] Define module-constant initialization, evaluation, mutability, and
      visibility.
- [ ] Define release-mode and bounds-check option semantics.

## Phase 0: Quarantine Unsafe Standard Library APIs

- [ ] Add a machine-readable status for every `std` module: usable, stub,
      unsafe, or platform-specific.
- [ ] Describe the experimental status and module classifications in
      `std/README.md`.
- [ ] Remove success-returning no-op networking functions.
- [ ] Remove or rename APIs described as atomic that use ordinary loads and
      stores.
- [ ] Fix growable-buffer overflow and allocation-failure handling.
- [ ] Preserve the original allocation when reallocation fails.
- [ ] Remove hard-coded pthread object sizes.
- [ ] Correct signed FFI return types for `read`, `write`, `send`, and `recv`.
- [ ] Add a frontend-check test for every `.fib` standard library module.
- [ ] Add runtime tests only for modules classified as safe.

## Phase 1: Complete Semantic Validation

The analyzer must reject invalid source before either backend runs.

### Numeric and Operator Rules

- [ ] Implement one implicit-conversion matrix for operators, assignments,
      arguments, and returns.
- [ ] Restrict implicit coercion to the locked widening rules.
- [ ] Implement an explicit-cast legality matrix for integers, floats, pointers,
      enums, and aggregates.
- [ ] Check integer literal ranges after contextual typing.
- [ ] Diagnose compile-time division and remainder by zero.
- [ ] Implement wrapping integer arithmetic consistently in both backends.
- [ ] Implement trapping float-to-integer conversion consistently in both
      backends.
- [ ] Add checked builtins only after documenting each builtin's behavior.
- [ ] Require `if` conditions to be `@bool`.
- [ ] Require loop conditions to be `@bool`.
- [ ] Restrict bitwise and shift operators to integers.
- [ ] Define the permitted pointer arithmetic operations.
- [ ] Reject all pointer operations outside the permitted set.
- [ ] Define legal equality operands, including pointer/null comparisons.

### `null`, `@never`, and Control Flow

- [ ] Type unconstrained `null` as `*@void` instead of `@never`.
- [ ] Restrict null assignment and comparison to pointer-like types.
- [ ] Stop coercing `null` to arbitrary target types.
- [ ] Restrict `@never` to function return positions.
- [ ] Prove that an `@never` function cannot return normally.
- [ ] Prove that every reachable path in a non-void function returns a value.
- [ ] Decide whether unreachable statements are errors or permitted.
- [ ] Enforce the chosen unreachable-statement policy.

### Assignment and Initialization

- [ ] Validate mutability for every multi-assignment target.
- [ ] Validate type compatibility for every multi-assignment target and value.
- [ ] Reject undeclared identifiers in multi-assignment targets.
- [ ] Validate mutability through nested fields and indexed lvalues.
- [ ] Track definite initialization across branches, loops, switches, break,
      continue, and return.
- [ ] Reject every read of a possibly uninitialized local.

### Enums, Generics, and Runtime Types

- [ ] Reject payload enum variants used without their payload constructor.
- [ ] Diagnose duplicate switch variants.
- [ ] Diagnose duplicate wildcard switch arms.
- [ ] Enforce exact total arity for generic function calls.
- [ ] Enforce compile-time generic argument arity and kinds.
- [ ] Enforce runtime argument arity after removing compile-time arguments.
- [ ] Resolve every qualified and unqualified type name during analysis.
- [ ] Detect type-alias cycles.
- [ ] Detect infinitely sized recursive value types.
- [ ] Prove that every runtime type has finite layout before lowering.
- [ ] Add one negative analyzer test for every semantic rule above.

## Phase 2: Verify IR and Repair Lowering

### Flat IR Verification

- [ ] Verify that labels are unique and defined.
- [ ] Verify that every block has exactly one terminator.
- [ ] Verify that no instruction follows a terminator.
- [ ] Verify that each temporary has one definition.
- [ ] Verify that temporary definitions dominate every use.
- [ ] Verify operand and instruction types.
- [ ] Verify call and return signatures.
- [ ] Run the verifier before consuming flat IR.

### LLVM Verification and Fallback

- [ ] Verify LLVM modules produced by the flat-IR route.
- [ ] Verify LLVM modules produced by the direct route.
- [ ] Report verifier failures as internal compiler errors.
- [ ] Introduce an explicit `Unsupported` result for flat-IR lowering.
- [ ] Fall back to direct lowering only after an `Unsupported` result.
- [ ] Preserve flat-IR consumer and LLVM failures instead of silently falling
      back.
- [ ] Add tests proving malformed IR cannot trigger fallback.

### Lowering Correctness

- [ ] Replace flat-IR short-circuit temporary reassignment with a PHI, block
      parameter, or addressable slot.
- [ ] Reject unsupported pointer operations before LLVM value conversion.
- [ ] Track direct-backend locals by resolved binding identity.
- [ ] Restore direct-backend bindings when each lexical scope exits.
- [ ] Predeclare every function signature before lowering any body in both
      routes.
- [ ] Stop emitting ordinary statements after a block terminator.
- [ ] Add forced-route differential tests for constructs supported by both
      routes.

## Phase 3: Add Canonical Package and Symbol Identity

### Package Namespaces

- [ ] Define a canonical `ModuleId` independent of import aliases.
- [ ] Represent qualified names as arbitrary path-segment sequences.
- [ ] Change expression type references from `{ module, member }` to qualified
      paths.
- [ ] Change value references from `{ module, member }` to qualified paths.
- [ ] Make `import std` load a package namespace instead of `std.fib`.
- [ ] Resolve child modules directly from package-relative physical paths.
- [ ] Remove the drop-first-segment import fallback after installed package
      discovery replaces `-I=std`.
- [ ] Add tests for deeply qualified values, calls, types, enum variants, and
      constructors.

### Declaration Identity and Linkage

- [ ] Assign each declaration an identity containing its defining `ModuleId`.
- [ ] Carry declaration identity through module exports and typed declarations.
- [ ] Carry declaration identity through typed calls and flat IR.
- [ ] Carry declaration identity through both LLVM lowering routes.
- [ ] Generate stable fully qualified linkage names for non-extern functions.
- [ ] Preserve declared foreign symbols for extern functions.
- [ ] Remove unqualified-name deduplication from `dedupe_declarations`.
- [ ] Diagnose declaration collisions instead of using last-write-wins.
- [ ] Add link-and-run tests for direct imports and aliases.
- [ ] Add link-and-run tests for selective and diamond imports.
- [ ] Add link-and-run tests for equal local names in different modules.

### Visibility

- [ ] Define declaration visibility syntax.
- [ ] Define module visibility syntax.
- [ ] Make imports non-re-exporting by default.
- [ ] Diagnose access to private modules and declarations.
- [ ] Make `std::sys` private.
- [ ] Add tests proving imports do not implicitly re-export declarations.

## Phase 4: Make Frontend Stages and Diagnostics Reliable

### Real Stage Emits

- [ ] Add a load-and-lex entry point that performs no parsing.
- [ ] Add a parse entry point that performs no import resolution or analysis.
- [ ] Make `--emit=lex` stop after lexing.
- [ ] Make `--emit=parse` stop after parsing.
- [ ] Add a test where lexing succeeds and parsing fails.
- [ ] Add a test where parsing succeeds and import resolution fails.
- [ ] Add a test where parsing succeeds and analysis fails.

### Parser Hardening

- [ ] Add one reusable delimited-list parser.
- [ ] Reject EOF before every required closing delimiter.
- [ ] Use the shared parser for arrays, tuples, arguments, and parameters.
- [ ] Use the shared parser for imports, fields, structs, and enums.
- [ ] Require commas between list elements.
- [ ] Allow one trailing comma consistently.
- [ ] Add a truncated-input regression test for every delimiter kind.

### Duplicate Declarations and Bindings

- [ ] Diagnose duplicate top-level declarations.
- [ ] Diagnose duplicate type aliases.
- [ ] Diagnose duplicate parameters.
- [ ] Diagnose duplicate declared struct fields.
- [ ] Diagnose duplicate enum variants and payload fields.
- [ ] Diagnose duplicate bindings in one lexical scope.
- [ ] Enforce and test the selected nested-scope shadowing behavior.
- [ ] Enforce and test the selected duplicate foreign-symbol policy.

### Determinism

- [ ] Stop deriving imported declaration order from `HashMap::values()`.
- [ ] Preserve source and topological module order or sort by canonical identity.
- [ ] Add repeated-build tests for stable typed output.
- [ ] Add repeated-build tests for stable LLVM IR while excluding intentional
      path metadata.

### Source Information and Diagnostics

- [ ] Add stable source IDs.
- [ ] Replace point-only spans with byte ranges.
- [ ] Retain every imported module's source text during resolution.
- [ ] Preserve source ranges through typed AST and flat IR.
- [ ] Attach imported-module ranges to analyzer diagnostics.
- [ ] Attach source ranges to backend diagnostics.
- [ ] Return one public diagnostic type from `compile_project` and
      `check_project`.
- [ ] Categorize lexical, name, type, configuration, backend, and toolchain
      errors accurately.
- [ ] Preserve underlying error sources in public diagnostics.
- [ ] Reduce accidental public API exposure from internal `pub mod`
      declarations.
- [ ] Add machine-readable diagnostics after categories and ranges stabilize.
- [ ] Decode string escapes exactly once.

## Phase 5: Implement the Locked `defer` Model

- [ ] Reject nested `defer` statements during analysis.
- [ ] Reject `return` inside deferred statements during analysis.
- [ ] Define whether `break` and `continue` are allowed inside deferred
      statements.
- [ ] Evaluate return expressions before emitting deferred work.
- [ ] Run deferred statements in reverse declaration order.
- [ ] Run lexical-block defers on normal fallthrough.
- [ ] Run exited-scope defers on return.
- [ ] Run exited-scope defers on break and continue.
- [ ] Preserve outer deferred frames across nested loops and switches.
- [ ] Make flat and direct lowering produce equivalent `defer` behavior.
- [ ] Add execution tests for fallthrough, return, break, and continue.
- [ ] Add execution tests for nesting and return-expression side effects.
- [ ] Update `docs/control-flow.md` to document lexical scope.

## Phase 6: Add Target-Aware ABI and Layout

- [ ] Add a target triple to compilation options and the CLI.
- [ ] Set the LLVM target triple on every module.
- [ ] Set the LLVM data layout on every module.
- [ ] Derive pointer width and `@usize` from target data.
- [ ] Derive slice layout from target data.
- [ ] Derive struct and tuple size and alignment from target data.
- [ ] Derive tagged-enum payload size and alignment from target data.
- [ ] Remove hard-coded 64-bit pointer and alignment assumptions.
- [ ] Add layout tests against LLVM target data.
- [ ] Implement C default argument promotions for variadic extern calls.
- [ ] Add ABI tests for promoted integer and floating variadic arguments.

## Phase 7: Add Generic Data Types

- [ ] Define syntax for generic type declarations.
- [ ] Represent generic type parameters in the AST and typed AST.
- [ ] Add generic struct instantiation.
- [ ] Add generic enum instantiation.
- [ ] Substitute type parameters recursively through fields and variants.
- [ ] Cache monomorphized data types by defining module and concrete arguments.
- [ ] Include the fully qualified defining module in monomorphized names.
- [ ] Detect recursive or infinitely sized generic instantiations.
- [ ] Add cross-module tests for `Result<T, E>`.
- [ ] Add cross-module tests for `Option<T>` and `Buffer<T>`.

## Phase 8: Add Ownership and Deterministic Destruction

- [ ] Define which types are copyable and which are move-only.
- [ ] Add automatic moves for owned values.
- [ ] Reject use after move.
- [ ] Add immutable borrows.
- [ ] Add mutable borrows.
- [ ] Reject conflicting borrows.
- [ ] Define returned-view lifetime validation.
- [ ] Implement the selected slice ownership and escape rules.
- [ ] Reject returning or storing slices of expired local storage.
- [ ] Add deterministic destructors.
- [ ] Add an explicit destruction-suppression escape hatch.
- [ ] Define destructor behavior for return, loops, switch, and `defer`.
- [ ] Make `File`, `String`, `BytesMut`, and writers move-only.
- [ ] Add negative move, borrow, and use-after-move tests.
- [ ] Add execution tests for destruction on every control-flow exit.

## Phase 9: Add Interfaces and Static Dispatch

- [ ] Define interface declaration syntax.
- [ ] Define `impl Interface for Type` syntax.
- [ ] Define receiver syntax and mutability.
- [ ] Represent interfaces and implementations in the AST and typed AST.
- [ ] Add interface conformance checking.
- [ ] Add generic constraints.
- [ ] Add compile-time method lookup and static dispatch.
- [ ] Generate qualified method symbols.
- [ ] Diagnose missing, duplicate, and mismatched methods.
- [ ] Defer dynamic interface objects until static dispatch is complete.

## Phase 10: Replace the libc-Based Standard Library

### Foundation Types

- [ ] Add `std::result::Result<T, E>`.
- [ ] Add `std::option::Option<T>`.
- [ ] Add owned immutable `Bytes`.
- [ ] Add owned growable `BytesMut`.
- [ ] Add owned UTF-8 `String` and borrowed `Str`.
- [ ] Add borrowed byte slices.
- [ ] Add borrowed `Path` and owned `PathBuf`.
- [ ] Make Unix paths byte-based and reject embedded NUL.
- [ ] Make conversion from Unix paths to UTF-8 strings fallible.

### Private Unix Layer

- [ ] Add private `std::sys::unix` modules.
- [ ] Declare only native calls used by public implementations.
- [ ] Translate `errno` immediately into public errors.
- [ ] Centralize retry-on-interruption behavior.
- [ ] Hide file descriptors and native constants from public types.
- [ ] Add C shims for `stat`, macros, and ABI-sensitive structures.
- [ ] Keep platform-specific details out of public types.

### Files and I/O

- [ ] Add move-only `File` and `OpenOptions` types.
- [ ] Add `Metadata`, `Permissions`, `DirEntry`, and `ReadDir` types.
- [ ] Add filesystem open, create, remove, and rename operations.
- [ ] Add metadata, canonicalization, and directory operations.
- [ ] Add `Reader`, `Writer`, and `Seek` interfaces.
- [ ] Implement `Reader`, `Writer`, and `Seek` for `File`.
- [ ] Add `read_exact`, `read_to_end`, `write_all`, and `copy`.
- [ ] Add reusable buffering primitives.
- [ ] Add `read(path)` returning owned bytes.
- [ ] Add `read_text(path)` with UTF-8 validation.
- [ ] Add safe `stdin`, `stdout`, and `stderr` APIs.
- [ ] Handle partial I/O, interruption, EOF, allocation failure, and flushing.
- [ ] Close files automatically on every control-flow exit.
- [ ] Test empty, short, and multi-buffer file reads.
- [ ] Test partial writes, interruption, EOF, and allocation failures.
- [ ] Test binary data, non-UTF-8 paths, and invalid UTF-8 text.
- [ ] Test temporary-directory operations and automatic file closure.

### Formatting and Migration

- [ ] Add formatting into `Writer`.
- [ ] Add an executable sample for each major standard library abstraction.
- [ ] Migrate every sample away from `std::libc`.
- [ ] Migrate every documentation example away from `std::libc`.
- [ ] Delete `std/libc.fib` in the same breaking change.
- [ ] Delete or replace the current incomplete standard library modules.
- [ ] Document the removal of `std::libc` in `CHANGELOG.md`.

### Later Modules

- [ ] Add process APIs.
- [ ] Add time APIs.
- [ ] Add stateful random generators.
- [ ] Add networking APIs.
- [ ] Add threads, mutexes, conditions, and real atomic operations.

## Phase 11: Add Native Libraries, C Unions, and X11

### Remaining Linker Support

- [ ] Add repeatable `-L`/`--library-path` CLI and library options.
- [ ] Add raw linker argument options.
- [ ] Add target and sysroot linker options.
- [ ] Preserve a configured `$CC` command and all of its arguments.
- [ ] Add a missing-library diagnostic test.
- [ ] Add a custom-library-search-path test.
- [ ] Add successful native-library linkage tests through the public API.

### C-Compatible Unions

- [ ] Implement the selected untagged `union` syntax.
- [ ] Enforce the selected initialization and active-field rules.
- [ ] Enforce the selected field-read and field-write safety rules.
- [ ] Enforce the selected by-value and pointer FFI restrictions.
- [ ] Derive union size and alignment from target data.
- [ ] Verify Fib union layouts against equivalent C unions.
- [ ] Keep untagged C unions distinct from tagged Fib enums.

### X11

- [ ] Add platform gating for `std::x11`.
- [ ] Add ABI-accurate extern declarations and opaque handle types.
- [ ] Add C shims for macros, callbacks, and `XEvent`.
- [ ] Add an X11 compile-and-link sample.
- [ ] Run the sample where a display server is available.
- [ ] Use compile/link-only CI where no display server is available.

## Phase 12: Improve Coverage, Performance, and Tooling

### Release Mode

- [ ] Separate optimization level from bounds-check policy in the public API.
- [ ] Add CLI conflict and default tests.
- [ ] Update README and CLI help.

### Tests

- [ ] Auto-discover `samples/*.fib`.
- [ ] Require each sample to declare exact-output, compile-only, or
      expected-failure policy.
- [ ] Convert `tests/probe_tmp.rs` output probes into assertions.
- [ ] Add lexer and parser no-panic fuzzing.
- [ ] Add randomized module-cycle tests.
- [ ] Add randomized type-cycle tests.

### Lowering Performance

- [ ] Hoist fixed-size local allocations into function entry blocks.
- [ ] Replace unnecessary aggregate spills and reloads with aggregate SSA.
- [ ] Borrow symbol tables during type mapping instead of cloning them.

### Cargo and Package Metadata

- [ ] Confirm unused direct dependencies with Cargo tooling.
- [ ] Remove confirmed unused direct dependencies.
- [ ] Add package description, repository, README, keywords, and categories.
- [ ] Add `rust-version` metadata matching the repository's MSRV policy.
- [ ] Move test-only dependencies to development dependencies where possible.

### LSP Frontend Integration

Begin this work only after canonical declaration identity in Phase 3 and source
information and diagnostics in Phase 4 are stable.

- [ ] Replace the JavaScript analysis path with a Rust `fib-lsp` crate that
      depends on `fibc` with default features disabled.
- [ ] Define one frontend document-analysis API that accepts a URI, source
      snapshot, include roots, and a cancellation token without invoking LLVM.
- [ ] Introduce a source-provider abstraction for entry files and imports.
- [ ] Make the source provider prefer versioned open-document snapshots and
      fall back to disk for unopened files.
- [ ] Key parsed and analyzed module caches by stable source ID, document
      version, and dependency versions.
- [ ] Invalidate cached dependants when an imported document changes.
- [ ] Return diagnostics for every source involved in an analysis with its
      canonical URI, byte range, severity, category, and optional remedy.
- [ ] Add parser synchronization and recovery at declaration and statement
      boundaries so one edit can produce multiple useful diagnostics.
- [ ] Continue name and type analysis on recoverable partial AST nodes without
      manufacturing valid types for erroneous expressions.
- [ ] Add tested conversion utilities between compiler byte ranges and LSP
      zero-based UTF-16 positions, including non-ASCII and multiline input.
- [ ] Retain complete ranges and stable node IDs for declarations, bindings,
      identifiers, expressions, and type expressions through semantic analysis.
- [ ] Build a per-document semantic index mapping source ranges to resolved
      declaration IDs and inferred types.
- [ ] Generate semantic tokens from compiler tokens and the semantic index
      instead of the heuristic scanner in `lsp/src/highlight.js`.
- [ ] Implement compiler-backed hover, completion, go-to-definition, references,
      and rename only after the semantic index is stable.
- [ ] Run frontend work off the protocol thread, debounce rapid edits, cancel
      obsolete analyses, and publish results only for the matching document
      version.
- [ ] Clear diagnostics on document close and when a newer successful analysis
      no longer reports an earlier error.
- [ ] Add protocol tests for unsaved entry files, unsaved imports, transitive
      imports, stale-result suppression, cancellation, and diagnostic clearing.
- [ ] Add Unicode protocol tests proving diagnostic and semantic-token ranges
      use UTF-16 units expected by LSP clients.
- [ ] Add an editor smoke test that starts `fib-lsp` without LLVM installed and
      checks diagnostics plus semantic tokens for a small project.
- [ ] Document installation, workspace-root discovery, include-path settings,
      supported capabilities, and current recovery limitations.

### Later Tooling

- [ ] Add a package manifest format.
- [ ] Add installed standard library discovery.
- [ ] Add `fibc run`.
- [ ] Add a formatter.
- [ ] Add editor tooling after semantic soundness stabilizes.
