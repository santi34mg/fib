# Binary and CLI

`main.rs` parses Clap commands and delegates to `cli::exec_command`.
The binary target is `fibc`; the Clap tool name is `fib`.

```sh
cargo run -- --help
cargo run -- build --single-file tests/programs/return_42.fib
./tests/programs/return_42  # exit status 42
```

`build --single-file PATH` loads, analyzes, lowers, verifies, emits an LLVM object
and links an executable beside the source, then prints its path. The current subset supports int32
helpers/direct calls, initialized int32/bool locals, assignment, comparisons and
nested if/else with early returns. Arithmetic and source loops remain unsupported. LLVM 21 and a
host `cc` linker are required. Without the LLVM feature, building reports an
explicit error. Help and frontend/IR library APIs need no LLVM.

Build without a single file (build.fib), `run` and `test` return errors. Historical
positional-file, include-root, check/emit/link/release and output flags are absent.
`tests/executable.rs` exercises real CLI builds and executable exit statuses.

The future default `fib build` path will discover exactly `<cwd>/build.fib`, lower
it to verified owned IR and interpret it to construct a validated `BuildPlan`; it
will not emit or load a native build-file artifact. See the
[interpreted build-file plan](../../roadmap/08-interpreted-build-files.md).
