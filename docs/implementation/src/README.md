# `src/` Compiler Map

This directory mirrors the Rust source root.

| Source | Ownership | Detailed page |
| --- | --- | --- |
| `main.rs` | Minimal binary entry; parse CLI and execute | [`cli.md`](cli.md) |
| `lib.rs` | Library facade and public re-exports | [`lib.md`](lib.md) |
| `cli.rs` | Clap schema, output printing, process exit | [`cli.md`](cli.md) |
| `diagnostics.rs` | User-facing diagnostic structure and rendering | [`diagnostics.md`](diagnostics.md) |
| `driver.rs` | Pipeline orchestration, modules, outputs, linking | [`driver.md`](driver.md) |
| `frontend/` | Lexing, parsing, semantic analysis | [`frontend/`](frontend/README.md) |
| `ir/` | Flat middle-end importer and representation | [`ir/`](ir/README.md) |
| `backend/` | LLVM lowering under the `llvm` feature | [`backend/`](backend/README.md) |

## Ownership Rule

- The frontend owns source-language validity.
- The driver owns stage orchestration, filesystem/module loading, output policy,
  and tool invocation.
- The IR owns a verifiable, backend-independent control-flow/value contract.
- The backend owns target representation and ABI details, not source-language
  type checking.
- Diagnostics should cross boundaries as structured errors; stages should not
  print directly.

Several current implementation gaps violate these ideal boundaries. They are
listed on the subsystem pages and in the
[`improvement roadmap`](../improvement-roadmap.md).
