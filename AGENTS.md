# AGENTS.md

Fib compiler (`fibc`) in Rust. Pipeline: `src/driver.rs` validate → read → lex/parse → `resolve_imports` → `analyze` → lower → emit/link. Library API in `src/lib.rs`: `compile_project` / `check_project`.

## Toolchain

- Pinned stable `1.98.0` via `rust-toolchain.toml` (also the MSRV policy — bump deliberately, never by drift). No `rustfmt.toml`/`clippy.toml`; defaults enforced by CI.
- Full builds need LLVM 21 (CI: `ZhongRuoyu/setup-llvm@v0`) + a C compiler on `$PATH`. Frontend-only work needs neither.

## Commands

Pre-push gate (CI order): `cargo fmt --check` → `cargo check` → `cargo clippy --all-targets -- -D warnings` → `cargo build` → `cargo test` + `cargo test --all-features`. Also: `cargo check --no-default-features`, `cargo test --no-default-features` (frontend job).

- `cargo test --lib` — unit tests only (works with `--no-default-features`).
- `cargo test --test e2e [name]` — e2e only; e.g. `cargo test --test e2e e2e_hello_world`. Needs `llvm` feature + clang; slow.
- Fast focused check without LLVM/clang: `cargo run --no-default-features -- <file> --check` or `--emit=lex|parse|typed`. Full CLI: `cargo run -- --help`.

## Running Fib programs

- `cargo run -- samples/hello_world.fib -I=std` → binary at `out/<stem>` (`out/` git-ignored, `*.ll` ignored); `-o` overrides, `--emit-llvm`/`--llvm-out <FILE>` keeps IR.
- `-I`/`--include-path` is repeatable; entry file's dir is always searched first. `import std::libc` with `-I std` resolves via the drop-first-segment fallback (`<root>/libc.fib`) — keep `-I=std`, do not "fix" the two-way search in `module_candidate_paths`.
- Linker: `--cc` > `$CC` > `clang-17` > `clang`. `-O 0|1|2|3|s|z`, `--release` skips debug OOB traps on `arr.[i]`/`arr.[a..b]` (constant OOB is always a compile error). `--check` overrides `--emit`; `lex|parse|typed` never touch LLVM/clang.

## Architecture notes

- `src/driver.rs` owns orchestration, module resolution (canonical-path interning dedupes alias spellings; diamond imports merged by `dedupe_declarations`, entry shadows imports), and clang linking. `src/cli.rs` is arg parsing + printing only. All errors funnel into one `CompilerError` (`src/diagnostics.rs`) — single print path.
- Middle-end `src/ir/`: `lower_typed_program` tries the flat `IrProgram` path (`ir_lower` consumer) first, falls back to direct `TypedProgram → LLVM` for structs/pointers/enum payloads/etc. Never silently miscompiles; which path is live is observable per program.
- `src/frontend/{lexer,parser,analyze}/` are split per-operator/per-concern mirroring each other; `src/backend/lowering/` is split per-concern (`context.rs` owns `FunctionLowering`, `types.rs` owns `map_type_to_llvm`).

## Tests & conventions

- `tests/e2e.rs` (`#![cfg(feature = "llvm")]`) compiles every `samples/*.fib` via the library API into fresh temp dirs and runs them; deterministic samples pin exact stdout, `fib_bench` is compile-only. Expected output is also in each sample's header comment.
- Unit tests use in-memory sources (`CompilationOptions { source_override: Some(..), project_path: "test.fib" }`) — no disk needed.
- New lowering branch → ship a `samples/*.fib` reproducer. Soundness fix → ship a `get_typed_err` negative test. For diagnostics assert `is_err()` + message substring/line, never the full message. Do not chase 100% coverage on lowering files; `audit`/`deny`/`tarpaulin` jobs are advisory (`continue-on-error`).
- Trunk-based on `main`; squash-merge with Conventional Commits PR title (`feat(frontend): …`, `fix(backend): …`); update `CHANGELOG.md` under `[Unreleased]`. CI skips docs-only changes (`docs/**`, `*.md`, `LICENSE`, `.gitignore`).
