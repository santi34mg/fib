# Samples: compilation and execution coverage

Every top-level [`samples/*.fib`](../../../samples) example builds and executes
with the current compiler and matching source std on Linux x86-64. See the
[sample guide](../../../samples/README.md) for commands, topics and exit policies.

`tests/samples.rs` discovers all top-level samples and compiles each through the
CLI with `--sysroot library`, preserving filenames in temporary directories. It
asserts exit status and exact stdout/stderr, with a ten-second execution timeout.
Each new sample must have an explicit expectation. The process-entry case supplies
`FIB_EXIT=42`; the recursive Fibonacci benchmark defaults to a bounded limit of 20.

```sh
LLVM_SYS_211_PREFIX=/usr/lib/llvm/21 cargo test --offline --test samples
cargo test --offline --no-default-features samples_parse_with_current_frontend
```

The parser discovery test remains syntax-only coverage and works without LLVM.
Existing driver and executable tests additionally cover individual samples and
their IR contracts. `deferred_blocks.fib` replaces the keyword-named `defer.fib`,
which could parse in isolation but failed CLI file-namespace validation.

Samples now use source std output with tagged formatting arguments, slice-based
sorting/allocation, explicit `comp` type bindings, and byte-slice string helpers.
`multiple_returns.fib` uses a struct return and multi-binding locals because native
tuple lowering remains unsupported. `memory.fib` names its std alias `mem` to avoid
self-namespace shadowing. The `specification_runtime` pointer-access warning is
intentional; `sizeof` checks the current target layout.
`const_bindings` demonstrates immutable scalar/struct/enum bindings and independent
mutable inline copies. `indexed_globals` uses writable global array storage.

[`example-project/`](../../../example-project/README.md) separately exercises a
build file, entry/output selection, generics, arrays, structs and enums. Its CLI
execution is covered by `tests/executable.rs`.

Preserve example operations when migrating syntax. Add positive and malformed
parser tests for grammar changes, and update execution expectations when intended
sample behavior changes. Do not treat parser success as runtime conformance.
