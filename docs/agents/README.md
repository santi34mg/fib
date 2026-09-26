# Compiler Implementation Guide

This tree is the maintained implementation map for Fib compiler contributors
and automated agents. It complements the user-facing language guide in
[`docs/`](../README.md) and replaces the dated audit files as the starting point
for current architecture work.

Reviewed against the repository on 2026-09-23.

## Pipeline

```text
src/main.rs
  -> src/cli.rs
  -> src/driver.rs
       validate and read
       -> lexer
       -> parser / AST
       -> recursive import resolution
       -> semantic analysis / TypedProgram
       -> TypedProgram -> IrProgram -> LLVM (core subset)
          or TypedProgram -> LLVM (fallback)
       -> emit LLVM / invoke clang
```

The two lowering routes both produce LLVM. They are not independent target
backends. The direct typed-AST route currently supports more language features.

## Repository Mirror

- [`repository.md`](repository.md): root manifests, toolchain, workflows, and directory ownership
- [`src/`](src/README.md): compiler pipeline and public API
- [`src/frontend/`](src/frontend/README.md): tokens, lexer, parser, AST, analyzer, and typed AST
- [`src/ir/`](src/ir/README.md): flat middle-end representation and importer
- [`src/backend/`](src/backend/README.md): LLVM feature boundary and lowering routes
- [`tests/`](tests/README.md): unit, integration, and end-to-end strategy
- [`samples/`](samples/README.md): executable language coverage
- [`std/`](std/README.md): standard-library status and safety concerns
- [`.github/workflows/`](.github/workflows/ci.md): CI contract
- [`roadmap/README.md`](docs/roadmap/README.md): prioritized language and compiler work

## Reading Rules

Every implementation page distinguishes these concepts:

- **Current behavior** describes what the checked-in code does now.
- **Invariant** describes a property a stage establishes or relies on.
- **Known risk** describes evidence-backed behavior that may be incorrect,
  incomplete, nondeterministic, or unsafe.
- **Improvement direction** is not a compatibility promise. Confirm it against
  tests and language documentation before implementation.

Source line numbers intentionally are not embedded here because they decay
quickly. Links point to files and name the relevant functions or types.

## Highest-Risk Boundaries

1. Module analysis resolves qualified names, but backend declaration flattening
   does not preserve stable module-qualified symbol identity.
2. Semantic analysis does not yet prove all assumptions made by lowering,
   including boolean conditions, legal casts/operators, initialization, and
   all-path returns.
3. The flat IR route silently falls back on every error, has no verifier, and
   has known SSA and capability mismatches with its LLVM consumer.
4. The direct LLVM route tracks storage by source name rather than resolved
   binding identity, which can violate lexical scoping under shadowing.
5. LLVM modules are printed without verification, and target layout is largely
   hard-coded for a 64-bit native environment.
6. Most standard-library modules are not exercised by CI and several expose
   stubs or platform-sensitive FFI as if they were complete.

Start correctness changes with the P0 section of the
[`improvement roadmap`](docs/roadmap/README.md).

## Updating This Tree

When changing a compiler subsystem:

1. Update its mirrored implementation page in the same change.
2. Update the support matrix or known risks if a stage boundary changes.
3. Move completed roadmap items to the changelog rather than leaving stale
   warnings here.
4. Add both positive and negative tests for new semantic invariants.
5. Prefer named items and relative source paths over line-number references.
