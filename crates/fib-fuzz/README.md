# Fib frontend fuzzer

`fib-fuzz` is a Rust coverage-guided genetic fuzzer. It tests tokenization,
parsing and strict single-file analysis with the primitive prelude and no std.
It never executes generated Fib programs. Compiler diagnostics are ordinary
outcomes; worker panics, abnormal exits and timeouts are findings.

## Build and run

Install matching coverage tools with `rustup component add llvm-tools-preview`.
Build a separately instrumented executable:

```console
env RUSTFLAGS='-C instrument-coverage' cargo build --offline -p fib-fuzz --target-dir target/fuzz-instrumented
env LLVM_PROFILE_FILE=target/controller-%p.profraw target/fuzz-instrumented/debug/fib-fuzz run --mode both --evaluations 10000 --seed 42 --output out/fuzz-run-42
```

These are direct executable invocations; no shell scripts are required. The
output directory must be new. The tool discovers `llvm-profdata` and `llvm-cov`
in the current Rust sysroot, falling back to PATH. Override them with
`FIB_FUZZ_LLVM_PROFDATA` and `FIB_FUZZ_LLVM_COV` when necessary. Startup performs
a real profile round trip and rejects missing instrumentation or incompatible
tools. This does not require Fib's LLVM backend feature.

`--mode chars` mutates Unicode characters, including control characters.
`--mode tokens` mutates actual token spellings, preserving quoted literals and
operators. Both modes support insertion, deletion, replacement, duplication and
20% crossover, followed by one to four mutations. Token rendering inserts spaces
and the frontend tokenizes the result again; grammatical validity is not required.

Populations evolve separately with three-way tournament selection and four
elites. Fitness ranks new covered frontend locations, population rarity, total
coverage and shorter source length. Ten percent of offspring are fresh random
inputs. `both` alternates evaluations and shares new corpus entries when they
can be represented in the other mode. Coverage is tracked independently.
The default population is 64; initial generations fill before parent selection.

Use `--corpus DIRECTORY` to import `.fib` files in sorted order. Token mode
imports only lexically representable sources. Defaults are 10,000 evaluations,
16,384 bytes, 2,048 tokens, and a 2,000 ms worker deadline. Override them with
`--evaluations`, `--population`, `--max-bytes`, `--max-tokens`, and `--timeout-ms`.
The seed must be nonzero and population at least four.

## Findings and replay

The run directory contains coverage-expanding `corpus-*.fib` files,
`crash-*.fib` / `timeout-*.fib` findings, bounded diagnostic captures, finding
metadata, `run.json` with arguments and worker fingerprint, and `summary.json`.
`input.fib` is saved before each worker starts, including on infrastructure errors.

```console
target/fuzz-instrumented/debug/fib-fuzz replay out/fuzz-run-42/crash-123.fib
target/fuzz-instrumented/debug/fib-fuzz minimize out/fuzz-run-42/crash-123.fib --output out/small.fib --mode chars
```

Replay and minimization do not require coverage tools and also work with a normal
`cargo build -p fib-fuzz` binary. Replay reports the outcome and diagnostics.
Minimization removes character or token chunks, reconfirming reductions twice
against the exit-status/panic-location signature or timeout. It refuses successful
inputs and existing output files. Different failures at the same panic location
can share a signature; review minimized findings before treating them as identical.

## Limits and validation

Stable source coverage uses covered segment locations rather than edge coverage
or execution-count buckets. Each evaluation spawns a process and exports an LLVM
profile, so this is substantially slower than an in-process edge-guided fuzzer.
Only `fib-frontend` locations contribute fitness. Generated code is not linked
or run; multi-file plans and std modules are outside this version.
Worker stdout/stderr are each capped at 16 KiB while excess output is drained.
Wall time is bounded; process memory is not currently capped. Runs are serial
and seeded, but timeout-dependent outcomes need not reproduce exactly.

Run `cargo test --offline -p fib-fuzz` for lexical round trips, seeded mutation
bounds, fitness ordering, isolated worker outcomes and replay CLI checks.
