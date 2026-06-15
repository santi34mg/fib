---
name: atommic-commits
description: Split the current working tree changes into multiple small, one-line commits grouped by area (lexer, parser, ast, analysis/hir, lowering, samples/std, docs, compiler.fib, etc). Use when the user asks to commit pending changes as several focused commits.
---

# Grouped commits

Split pending changes into several focused, one-line commits instead of one big commit.

## Steps

1. Run `git status --porcelain` and `git diff --stat` to see all changed/untracked files.
2. Run `git diff -- <files>` per area to understand *what* changed, not just which files.
3. Group files by area of the codebase. Typical groups for this project, in commit order:
   - `src/lexing/`, `src/tokens/` — lexer
   - `src/ast.rs`, `src/parsing/` — parser/AST
   - `src/hir.rs`, `src/analysis/` — analysis/HIR
   - `src/lowering/` — codegen/lowering
   - `samples/`, `std/`, `hello_world.fib` — sample programs and stdlib
   - `docs/`, `README.md` — documentation
   - `compiler.fib` — self-hosted compiler, commit last
   Adjust groups to fit whatever actually changed — don't force unrelated files together.
4. For each group, in order: `git add <files>` then `git commit -q -m "<scope>: <one-line summary>"`.
   - Commit messages are a single line, imperative, prefixed with the area (e.g. `lexer:`, `parser:`, `docs:`).
   - Each commit must build logically on its own (e.g. lexer changes before the parser changes that depend on them).
5. After all commits, run `git log --oneline -n <count>` and `git status` to confirm a clean tree.

Never use `git add -A` or `git add .` — always add explicit files/paths per group.
