# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project overview

Fib is a systems programming language with a compiler written in Rust (binary name `fibc`). The compiler currently lexes, parses, analyzes, and lowers to LLVM IR, then invokes clang to produce a native binary. A self-hosted compiler (`compiler.fib`, written in Fib itself) is in progress.

## Common commands

```bash
# Build
cargo build

# Run tests
cargo test

# Compile and run a Fib program (the `-I=std` flag adds the stdlib search path)
cargo run -- samples/hello_world.fib -I=std
./out/hello_world
```

The compiler requires `clang-17` on `$PATH` (falls back to unversioned `clang` if `clang-17` is missing) when using the LLVM backend (the default, enabled via the `llvm` cargo feature).

Output artifacts (`.ll` IR files and binaries) are written to `out/`.

## Architecture

The compiler pipeline (driven by `src/driver.rs::compile`):

1. **Lexing** (`src/lexing/`) — source text -> `Vec<Token>` (`src/tokens/`, with submodules for keywords, operators, punctuation, literals, identifiers, and `@`-prefixed builtins).
2. **Parsing** (`src/parsing/parser.rs`) — tokens -> `Ast` (`src/ast.rs`).
3. **Module resolution** (`src/driver.rs::resolve_module`) — recursively resolves `import` declarations by searching the source file's directory and any `-I` include paths (e.g. `std`), lexing/parsing/analyzing each imported module into an `HIRModule` with exported symbols.
4. **Analysis** (`src/analysis/generate_hir.rs`) — `Ast` + resolved imported modules -> `CompilationUnit` (HIR, `src/hir.rs`), doing type checking/resolution. Produces `declarations` (local) and `imported_declarations` (pulled in from imports).
5. **Lowering** (`src/lowering/llvm_lower.rs`, gated by the `llvm` feature) — HIR -> LLVM IR text via `inkwell`.
6. **Codegen finish** — IR is written to `out/<name>.ll`, then compiled to a binary at `out/<name>` via clang.

Key types:
- `Ast` / `DeclarationNode` (src/ast.rs) — parsed syntax tree.
- `CompilationUnit` / `HIRModule` / `HIRDeclaration` / `Scope` (src/hir.rs) — resolved, type-checked representation used for lowering.
- `FrontendResponse` (src/driver.rs) — bundles tokens, parse errors, analysis errors, AST, and HIR; used for tooling/diagnostics without requiring the LLVM feature.

The `llvm` feature is on by default; lowering and `compile_project`/`exec_command` are only available when it's enabled, so frontend-only work (lexer/parser/analysis) should not assume LLVM is present.

## Standard library and samples

- `std/` — the Fib standard library (`std/core`, `std/io`, `std/fs`), resolved via `-I=std`.
- `samples/` — example `.fib` programs demonstrating language features end-to-end.
- `docs/` — per-feature language reference (types, operators, control flow, functions, structs, enums, switch, arrays, pointers/memory, imports, casting, generics, builtins). Start at `docs/README.md`.

## Self-hosted compiler

`compiler.fib` is a work-in-progress self-hosted compiler written in Fib (currently implementing a lexer). It is a living document tracking the bootstrapping effort, not a finished component.
