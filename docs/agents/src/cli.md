# `src/main.rs` and `src/cli.rs`

## Responsibility

`main.rs` delegates to `cli::parse_args` and `cli::exec_command`. `cli.rs` owns
the Clap argument schema, conversion to `CompilationOptions`, printing stage
outputs, printing the single final diagnostic, and process exit status.

## User Options

The CLI accepts an entry file, repeatable include roots, output paths, an emit
kind, check-only mode, retained LLVM output, compiler selection, optimization
level, and release mode. `--check` forces the effective stage to typed analysis.
`--release` currently means "omit dynamic bounds traps"; it is not a complete
release profile and does not imply optimization.

## Output Contract

- lex, parse, and typed outputs are debug-oriented text.
- LLVM and binary emits report output paths through `CompileOutput`.
- compilation failures print once to stderr and exit with status 1.
- Clap owns malformed-command-line behavior.

## Known Risks

- Lex/parse emits are named as stage cutoffs but the driver still runs all
  frontend stages.
- Typed debug output contains hash maps and is not guaranteed deterministic.
- There are no subprocess tests asserting stdout/stderr and exit behavior.
- Irrelevant or conflicting options are generally accepted rather than
  diagnosed before compilation.
- `--release` is easy to misread as an optimization/production umbrella.

## Tests

Unit tests in `src/cli.rs` exercise argument parsing. Add process-level tests for
help/version, every emit, parse and analysis failures, missing LLVM/compiler,
link failures, path conflicts, and exit codes.
