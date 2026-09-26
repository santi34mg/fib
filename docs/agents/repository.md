# Repository Map

This page mirrors the repository root and records the role of non-source
assets.

| Repository path | Responsibility | Implementation notes |
| --- | --- | --- |
| `Cargo.toml` | Package, feature, and dependency declaration | The `llvm` feature is enabled by default and gates Inkwell. Several direct dependencies appear unused and should be verified before removal. |
| `Cargo.lock` | Reproducible Rust dependency graph | Keep synchronized with deliberate manifest changes. |
| `rust-toolchain.toml` | Rust/MSRV pin | Rust 1.98.0 with rustfmt and clippy. A bump is a policy change. |
| `src/` | Compiler library and binary | See [`src/`](src/README.md). |
| `tests/` | Integration and LLVM e2e tests | See [`tests/`](tests/README.md). |
| `samples/` | Runnable feature examples | See [`samples/`](samples/README.md). |
| `std/` | Fib standard-library experiments and libc declarations | See [`std/`](std/README.md). |
| `docs/` | User language guide, historical audits, implementation guide | Historical `docs/audit/` pages are snapshots, not current contracts. |
| `.github/workflows/ci.yml` | Hosted validation | See [CI](.github/workflows/ci.md). |
| `.githooks/pre-commit` | Local full gate | Installation is not automatic; it is intentionally more expensive than a typical pre-commit hook. |

## Build Model

Fib is one Rust package with a library and binary. Default builds require LLVM
21 through Inkwell and a clang-compatible tool capable of consuming textual
LLVM IR. `--no-default-features` keeps the frontend, driver, diagnostics, and
flat IR importer available without LLVM.

The current "project" model is an entry `.fib` file plus recursive imports and
include roots. There is no package manifest, dependency resolver, target model,
or installed-standard-library discovery.

## Dependency Notes

Observed runtime dependencies are `clap`, optional `inkwell`, and `tempfile`.
The manifest also declares `serde`, `toml`, `regex`, `walkdir`, `petgraph`,
`either`, and `indextree`; no direct source use was found during the 2026-09-23
exploration. Confirm with compiler tooling before removing them because feature
or generated-code uses can be non-obvious.

## Portability Boundary

The generated ABI assumes a native 64-bit target in several places:

- `@usize`, pointers, function pointers, and slice lengths are modeled as 64-bit.
- Slice layout is treated as `{ ptr, i64 }`.
- tagged-union payload layout is computed manually.
- runtime bounds diagnostics call libc/POSIX functions.
- linking has no target, sysroot, `-L`, `-l`, or arbitrary linker-argument API.

Treat cross-compilation and broad platform claims as unsupported until target
triple/data-layout ownership and native linker configuration are explicit.
