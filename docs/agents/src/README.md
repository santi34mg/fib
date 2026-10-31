# Source ownership

See the [workspace crate map](../../../crates/README.md) for compiler dependency boundaries.

| Source | Current role |
| --- | --- |
| `src/main.rs`, `src/cli.rs` | CLI entry and build commands |
| `src/lib.rs` | Public `fibc` facade over workspace crates |
| `src/plan.rs` | Build stages and mutable build-file planning/finalization |
| `src/driver.rs`, `src/driver/` | Parsing entry points, analysis, entry adaptation and pipeline orchestration |
| `src/ir_interpreter.rs` | Restricted build-file IR interpreter |
| `crates/fib-builtins/src/lib.rs` | Stage-independent builtin identities |
| `crates/fib-frontend/src/frontend/` | Tokens, AST, parsing, names, typing, flow validation and editor services |
| `crates/fib-frontend/src/plan.rs` | File/package identities, immutable source plans and single-file loading |
| `crates/fib-frontend/src/standard_library.rs` | Installed source std and plan preparation |
| `crates/fib-frontend/src/diagnostics.rs`, `numeric.rs` | Diagnostics and shared scalar semantics |
| `crates/fib-ir/src/ir/` | Owned SSA, transactional builder and verifier |
| `crates/fib-lowering/src/lowering.rs`, `lowering/` | Validated analyzed-source to SSA lowering |
| `crates/fib-backend/src/backend/` | LLVM emission and target/ABI classification |

Frontend establishes source validity; the driver owns filesystem and pipeline
orchestration; IR owns backend-independent runtime identities/types; lowering
bridges frontend and IR; backend owns target representation. Cross-stage tests
live in `tests/frontend_lowering.rs` rather than making frontend depend on lowering.

Details: [CLI](cli.md), [library](lib.md), [driver/build](driver.md),
[frontend](frontend/README.md), [IR](ir/README.md),
[backend](backend/README.md), [diagnostics](diagnostics.md).
