# Source ownership

| Source under `src/` | Current role |
| --- | --- |
| `main.rs`, `cli.rs` | Clap entry and command schema; dispatch is `todo!()` |
| `lib.rs` | Public modules and `CompilerError` re-export |
| `build_plan.rs` | File IDs/names, source-file records, build plan; loader stub |
| `builder_object.rs` | Build stages/state; build-file discovery attempt, remaining operations `todo!()` |
| `driver.rs`, `driver/compiler_output.rs` | Pipeline boundaries, output enum; orchestration errors explicitly |
| `frontend/` | Working tokenizer/parser and scope utilities; annotated syntax and partial analysis with unfinished name/type passes |
| `ir/` | Self-owned SSA records; builder/verifier sketches |
| `ir_interpreter.rs` | Build-file interpreter `todo!()` |
| `backend/` | IR-only backend sketch, available without LLVM |
| `diagnostics.rs` | Structured rendering and parser-error conversion |

Details: [CLI](cli.md), [library](lib.md), [driver/build](driver.md),
[frontend](frontend/README.md), [IR](ir/README.md),
[backend](backend/README.md), [diagnostics](diagnostics.md).

Future ownership: frontend establishes source validity; driver owns filesystem
and stage orchestration; IR owns backend-independent runtime identities/types;
backend owns target/ABI representation. These are intended boundaries, not
claims that the missing stages execute today.
