# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- End-to-end regression net (`tests/e2e.rs`): compiles every `samples/*.fib`
  through the library API and executes the result, with exact-stdout asserts
  for deterministic samples (`fib_bench` is compile-only).
- IR middle-end (`src/ir/`): `SymbolId`-based 3-address IR with
  `lower_typed_program`, pretty-printing, and contract tests.
- Staged driver/CLI (`src/driver.rs`, `src/cli.rs`): `--emit=lex|parse|typed|llvm|bin`,
  `--check` frontend-only mode, `--emit-llvm`/`--llvm-out`, `--cc`, `-O`,
  `-I` include paths, module dedup.
- Compiler audit series (`docs/audit/01`-`06`): architecture, reliability,
  driver/CLI, frontend, testing, project health.
- CI hardening: `fmt` + `frontend (--no-default-features)` + LLVM
  `build_and_test` + advisory `cargo audit` jobs; docs-only changes skip CI.

### Changed
- Split `ir`, `analyze`, and lowering into modules mirroring the parser layout.
- Frontend: generic binary-expression parser; lexer split (`frontend/lexer/`);
  `BinOp` decoupled from lowering.
- Reliability: eliminated panics in lowering paths; closed assignment
  soundness holes (negative `get_typed_err` tests).
- Unsigned handling: `zext` widening, `udiv`/`urem`/`ucmp` opcode selection
  keyed off operand type; extracted `map_type_to_llvm` into `lowering/types.rs`.

### Removed
- Dropped the self-hosted compiler effort (`compiler.fib`, 518 lines) to
  focus on the Rust/LLVM compiler.
