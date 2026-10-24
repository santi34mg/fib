# Source ownership

| Source under `src/` | Current role |
| --- | --- |
| `main.rs`, `cli.rs` | Clap entry; single-file scalar/control-flow builds and narrow interpreted build files; run/test error |
| `lib.rs` | Public modules and `CompilerError` re-export |
| `plan.rs` | File IDs/names, source-file records, build plan and single-file loader |
| `builder_object.rs` | Build stages/state; build-file discovery attempt, remaining operations `todo!()` |
| `driver.rs`, `driver/compiler_output.rs` | Analysis and pipeline orchestration; shared SSA lowering lives in `ir/lowering.rs` |
| `frontend/` | Working tokenizer/parser, names, semantic typing and members; explicit metatype specialization, bounded type-return evaluation and initialization/control-flow completion |
| `ir/` | Analyzed-source scalar/struct/control-flow lowering, owned SSA, transactional builder and typed memory verifier |
| `ir_interpreter.rs` | Narrow constant-byte build API interpreter; general CFG explicitly rejected |
| `backend/` | Multi-block LLVM emitter with per-function ABI classification |
| `diagnostics.rs` | Structured rendering and parser-error conversion |

Details: [CLI](cli.md), [library](lib.md), [driver/build](driver.md),
[frontend](frontend/README.md), [IR](ir/README.md),
[backend](backend/README.md), [diagnostics](diagnostics.md).

Future ownership: frontend establishes source validity; driver owns filesystem
and stage orchestration; IR owns backend-independent runtime identities/types;
backend owns target/ABI representation. These are intended boundaries, not
claims that the missing stages execute today.
