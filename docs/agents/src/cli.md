# Binary and CLI

The [language specification](../../language-specification.md) is implemented for prelude names, casts, shallow readonly views,
warned pointer field access and exhaustive switches. Concrete generic functions,
multiple source files and fixed-array values execute. Package-rooted lookup, scoped
visibility, installed std sources and Linux x86-64 syscalls execute. Recursive source
discovery remains deferred. Checked runtime indexing/slicing and constant-initialized
globals execute; value-returning `comp fn` execution remains scaffolding.

`main.rs` parses Clap commands and delegates to `cli::exec_command`.
The binary target is `fibc`; the Clap tool name is `fib`.

```sh
cargo run -- --help
cargo run -- build --single-file tests/programs/return_42.fib
./tests/programs/return_42  # exit status 42
```

`build --single-file PATH` loads, analyzes, lowers, verifies, emits an LLVM object
and links an executable beside the source, then prints its path. The current subset supports fixed-width numeric/bool
helpers/direct calls, structs/typed pointers, partial initialization, stack
storage, field/pointer mutation, assignment, arithmetic/casts, comparisons,
nested if/else, while loops, break/continue and early returns. LLVM 21 and a
host `cc` linker are required. Without the LLVM feature, building reports an
explicit error. Help and frontend/IR library APIs need no LLVM.

`build --build-file PATH` supports the narrow interpreted build API. Default
build-file discovery, `run` and `test` return errors. Historical
positional-file, include-root, check/emit/link/release and output flags are absent.
`tests/executable.rs` exercises real CLI builds and executable exit statuses.

Native interoperability uses `callconv "C"` declarations. Repeatable
`--c-source PATH` options compile C helpers with host `cc`; `-l`/`--link-lib LIB`
and `-L`/`--library-path PATH` add native libraries and search paths. These options
work with either explicit build route. External C ABI support currently targets
Linux x86-64; see the [backend contract](../../../crates/fib-backend/src/backend/README.md).

The future default `fib build` path will discover exactly `<cwd>/build.fib`, lower
it to verified owned IR and interpret it to construct a validated `BuildPlan`; it
will not emit or load a native build-file artifact. See the
[interpreted build-file plan](../../roadmap/08-interpreted-build-files.md).
