# CONTRIBUTING

If you are reading this it means that you are probably interested in contributing to the language.
I am glad!

## How to contribute?

Feel free to create issues and discussions.

Contributions must be made through a comprehensive pull request to the `main`
branch (trunk-based; CI runs on `main` only).

## Coding conventions

Run these before pushing (CI enforces them):

- `cargo fmt --check` (or `cargo fmt` to fix)
- `cargo clippy --all-targets -- -D warnings`
- `cargo check` / `cargo check --no-default-features`

No `rustfmt.toml`/`clippy.toml` — defaults are enforced by CI.

## Testing

- Use the pinned Rust 1.98.0 toolchain. `cargo test --no-default-features`
  runs the current unit tests without LLVM; this includes frontend tests and
  architecture/data-contract tests, not just frontend code.
- Default/all-feature commands require LLVM 21 dependencies; backend emission
  remains unimplemented even when the dependency build succeeds.
- `cargo test --no-default-features frontend::parser` and
  `cargo test --no-default-features frontend::tokenizer` focus syntax tests.
- `cargo test --no-default-features samples_parse_with_current_frontend`
  discovers and parses all top-level `samples/*.fib` files. This checks syntax,
  not semantic analysis or program execution; keep sample intent intact.
- Run `npm test` from `lsp/` for the JavaScript highlighting/server tests.
- Conventions for new code:
  - Add positive and malformed-input tokenizer/parser tests for syntax changes.
  - Add real positive and negative tests when implementing unfinished operations;
    never replace explicit failures with successful empty results.
  - For diagnostics, assert on `is_err()` plus a message substring/line —
    not the exact full message, which is still evolving.
- Coverage (`cargo tarpaulin`, advisory CI job, non-blocking): use it to
  find untested code, not to gate merges. Do not chase 100% line coverage
  on the lowering files — it incentivizes tautological tests.
