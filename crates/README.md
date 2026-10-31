# Compiler crates

The workspace separates compiler stages so Cargo can cache them independently.
All packages are default members: commands run at the repository root continue
to check/test the complete compiler. The root `fibc` package keeps its binaries,
pipeline orchestration, build-file interpreter and public module facade.

| Package | Responsibility | Compiler dependencies |
| --- | --- | --- |
| `fib-builtins` | Builtin identities and names | None |
| `fib-frontend` | Tokens, AST, parser, analysis, diagnostics, scalar semantics, source plans, installed std and editor services | `fib-builtins` |
| `fib-ir` | Owned SSA, transactional construction and verification | `fib-builtins` |
| `fib-lowering` | Validated analysis to SSA, including entry selection | `fib-frontend`, `fib-ir` |
| `fib-backend` | Target/ABI classification and optional LLVM emission | `fib-ir` |
| `fibc` | Public facade, driver, build-file planning/interpreter, CLI, LSP and installer | Frontend, IR, lowering, backend |

Frontend and IR do not depend on one another or LLVM. Backend does not depend on
frontend or lowering. Builtin semantic signatures live in frontend, with shared
identities in `fib-builtins`. Source-plan records live in frontend; the mutable
build-file `PlanBuilder` stays with driver orchestration in the root package.
`CompilationUnit` is owned by frontend and re-exported from `fibc::driver`.

`fib-lowering` converts the analyzed Fib AST into SSA IR. It is separate from
`fib-ir` because it needs frontend types, while the IR representation, builder
and verifier do not. Putting this conversion inside `fib-ir` would make IR tests
compile the frontend and give the backend a transitive frontend dependency.
The separate crate lets frontend edits leave IR compilation cached, and lets
IR and backend tests run without compiling the frontend.

SSA IR to LLVM IR and object code belongs to `fib-backend`. That conversion is
distinct from the source-to-SSA conversion in `fib-lowering`.

```sh
cargo test -p fib-frontend
cargo test -p fib-ir
cargo test -p fib-backend --features llvm
cargo check -p fib-lowering
cargo test --workspace --no-default-features
cargo test --workspace --all-features
```

The facade retains paths such as `fibc::frontend::analyzer`, `fibc::ir::Ir`,
`fibc::ir::lowering::lower` and `fibc::backend::emit_llvm`. Tests requiring both
analysis and lowering live in `tests/frontend_lowering.rs`, keeping frontend's
own unit tests independent of later stages. Builtin signature conversion is now
the frontend helper `builtins::semantic_type(identity, types)`.

For an edit in one stage, test that package first, then validate the workspace.
`cargo test -p fibc` selects only the facade's tests, not dependency unit tests.
Release/bench profiles and generated Fib program semantics are unchanged.
