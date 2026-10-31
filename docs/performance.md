# Compiler and LSP performance

The original 2026-10-29 investigation below records measurements and optimization
candidates before implementation changes. Follow-up changes and measurements
are recorded separately. Benchmarks live in `benches/performance.rs` and use only
existing public compiler APIs plus the real `fib-lsp` stdio protocol. No compiler
or LSP implementation was modified for the original investigation.

## Follow-up: shared analysis snapshots and verifier workspace (2026-10-30)

Two optimizations now implement parts of candidates 1 and 2 below:

- `TypeState` shares immutable declaration recipes, function AST snapshots and
  diagnostic paths through `Arc<HashMap<...>>`. Recovery still copies/restores
  mutable inference state and `SemanticTypes`; generic evaluation still clones
  individual source functions before annotating them.
- `verify_ir` allocates its program-indexed predecessor and dominator tables once
  per call. Each function's CFG contents are released after its checks. All
  whole-program ownership, dominance and active-enum-variant checks remain.
  Function builders still clone and verify their whole staged IR.

The following are new local comparisons against the workspace checkout directly
before these two changes, rather than the older October 29 baseline. Both builds
use optimized no-default-feature binaries and the same fixtures/source std
(10 files, 31,721 bytes). The baseline benchmark executable was preserved before
rebuilding. Runs were sequential, with 10 samples and three warmups per case;
no compiler builds or test jobs ran during these recorded measurements. CPU
affinity/frequency were not fixed, and this is one local comparison, not a
statistical significance claim. The existing method/timing boundaries apply.

Medians in milliseconds:

| Helpers | Std | Collecting before | After | Lowering before | After | Verify before | After |
| ---: | :---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 10 | off | 0.978 | 0.217 | 0.361 | 0.313 | 0.021 | 0.019 |
| 10 | on | 189.542 | 50.968 | 44.133 | 40.339 | 1.600 | 1.393 |
| 100 | off | 72.322 | 2.571 | 18.890 | 13.738 | 0.320 | 0.169 |
| 100 | on | 451.445 | 69.478 | 95.223 | 72.095 | 2.298 | 1.566 |
| 500 | off | 2405.565 | 25.813 | 890.844 | 312.937 | 4.319 | 0.834 |
| 500 | on | 3241.190 | 168.199 | 1222.937 | 445.939 | 7.876 | 2.291 |

At 500 helpers, collecting analysis improved about 93x without std and 19x with
std. Lowering improved about 2.85x and 2.74x respectively, even though its
whole-program builder transactions remain. Collecting analysis and lowering
still grow faster than source size; mutable checkpoint copying and repeated
builder work remain candidates. These runs do not establish end-to-end CLI or
LSP latency improvements.

Raw CSV: [collecting before](benchmarks/2026-10-30-collecting-before.csv),
[IR before](benchmarks/2026-10-30-ir-before.csv),
[both changes after](benchmarks/2026-10-30-shared-state-after.csv).
Reproduce the selected cases on each implementation with:

```sh
FIB_BENCH_SAMPLES=10 FIB_BENCH_FILTER=analyze_collecting,lower_including_validation,verify_ir cargo bench --offline --no-default-features --bench performance
```

Validation after both changes: 798 no-default-feature tests, 870 LLVM-enabled
tests, strict all-target Clippy in both configurations, formatting and whitespace
checks passed. Existing tests cover collecting-error recovery, failed generic
instance rollback, ownership/dominance rejection, enum proofs and transactional
builder failure. Benchmark fixtures also completed successfully.

## LSP follow-up (2026-10-30)

The current release server was rebuilt and measured over real stdio with 30
samples plus three warmups per case. All diagnostic/completion fixture assertions
passed. No builds or test jobs ran concurrently with these measurements. Raw
results include min, p50, p95 and mean in [the stdio CSV](benchmarks/2026-10-30-lsp.csv).

Times below are milliseconds, shown as **p50 / p95**:

| Open standalone documents | Edit to diagnostics | Edit to completion | 10 edits to completion | Cached completion | Cached semantic tokens |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 57.248 / 59.870 | 47.535 / 50.248 | 49.554 / 96.825 | 0.036 / 0.106 | 0.023 / 0.031 |
| 5 | 285.987 / 289.909 | 277.260 / 283.481 | 551.881 / 566.245 | 0.025 / 0.048 | 0.018 / 0.095 |
| 20 | 1169.295 / 1276.812 | 1168.093 / 1179.495 | 2317.710 / 2348.177 | 0.030 / 0.127 | 0.021 / 0.073 |

Initialization was 1.195 ms p50 / 1.492 ms p95. The separate five-file explicit
project measured 178.050 ms p50 / 179.283 ms p95 for edit-to-diagnostics. That
project has fewer total functions than the standalone cases and includes build
planning; it is not a controlled grouping comparison.

Compared with the October 29 historical results, median edit-to-diagnostics
latency is about 40–41% lower across all three standalone document counts.
Twenty-document edit-to-completion fell from 1968 to 1168 ms; its ten-edit burst
fell from 3920 to 2318 ms. These are historical observations, not an isolated
before/after test of snapshot sharing: source std grew from 8 files / 23,461
bytes to 10 files / 31,721 bytes and the checkout includes other changes. CPU
frequency/affinity were not fixed. The one-document burst median is near one
analysis duration but its p95 approaches two; do not assume every burst is
coalesced into one job.

Repeated standalone analysis remains the main visible bottleneck: twenty open
documents cost about 20x the single-document diagnostic latency, while cached
completion stays below 0.04 ms median. The results support investigating shared
unchanged inputs and cancellation of obsolete work next. They do not imply
equivalent cached-completion latency on much larger source files.

The [direct service probes](benchmarks/2026-10-30-lsp-service.csv), run separately
with 30 samples, confirm that distinction. Dividing each batch median by its
request count gives 0.000316 ms/request for the tiny unfinished expression,
0.024209 ms after 10 helpers, 0.240347 ms after 100 helpers and 1.216833 ms after
500 helpers. These exclude stdio and reanalysis; the tiny case batches 1,000
requests and the others batch 100.

```sh
cargo build --offline --release --no-default-features --bin fib-lsp
FIB_BENCH_SAMPLES=30 FIB_BENCH_FILTER=lsp_stdio cargo bench --offline --no-default-features --bench performance
FIB_BENCH_SAMPLES=30 FIB_BENCH_FILTER=lsp_service cargo bench --offline --no-default-features --bench performance
```

## Reproduce

Run from the repository root, with the pinned Rust toolchain and matching local
std sources. No new benchmark dependencies are required.

```sh
cargo build --offline --release --no-default-features --bin fib-lsp
cargo bench --offline --no-default-features --bench performance
```

LLVM-enabled runs also measure LLVM emission, object generation, and a fresh
compiler process building and linking an executable. Build matching release
binaries first:

```sh
LLVM_SYS_211_PREFIX=/usr/lib/llvm/21 cargo build --offline --release --bins
LLVM_SYS_211_PREFIX=/usr/lib/llvm/21 cargo bench --offline --bench performance
```

The harness prints CSV with sample count, minimum, median (p50), p95, and mean in
milliseconds. `FIB_BENCH_SAMPLES` selects the count (default 30, plus three untimed
warmups per benchmark). `FIB_BENCH_FILTER` selects comma-separated substrings:

```sh
FIB_BENCH_SAMPLES=10 FIB_BENCH_FILTER=compiler/100 cargo bench --offline --no-default-features --bench performance
FIB_BENCH_SAMPLES=10 FIB_BENCH_FILTER=lsp_stdio cargo bench --offline --no-default-features --bench performance
FIB_BENCH_FILTER=clone_ir,verify_ir cargo bench --offline --no-default-features --bench performance
```

`FIB_BENCH_LSP` can select a different server executable; otherwise the harness
uses `fib-lsp` next to the benchmark profile's binaries. Ensure that binary was
built from the same checkout. Filters omit timed cases, but compiler fixture
preparation still runs. Full runs take several minutes because they deliberately
exercise the slow collecting-analysis and many-open-document paths. Benchmarks
do not impose timing gates on tests; invocations without Cargo's `--bench`
argument (including `cargo test --all-targets`) skip measurements.

## Method and scope

Measurements were taken on 2026-10-29 using optimized Cargo bench/release builds,
Rust 1.98.0, Linux 6.18.58_1, and an AMD Ryzen 7 7435HS (8 cores, 16 threads),
with approximately 15.3 GiB RAM. CPU boost was enabled; affinity and frequency
were not fixed. The working tree already contained compiler and std changes,
including generic maps/process entry and LSP completion. These results describe
that working tree, not an unmodified historical commit.
The base commit was `9ffc29b0db5be3678782d753a636db654d406a6e`; source std comprised
eight files totaling 23,461 bytes.
Release/bench optimization refers to the Rust compiler/server binaries; generated
Fib objects use the existing backend settings, including target-machine
`OptimizationLevel::None`. No optimization passes were added.

Compiler fixtures contain 10, 100, or 500 scalar helper functions, each with a
local binding, arithmetic, comparison, two return branches, and a small `main`.
Their source sizes are 1,020, 9,930, and 49,930 bytes. Every helper is analyzed
and lowered, although `main` calls only the first. Each size runs both with and
without the complete source std. Existing `generic_map` and `process_entry`
samples provide additional real-source analysis workloads.

Application text is supplied through in-memory overlays except for the actual
CLI build. Std files are read from disk with the normal source provider. These
are warm filesystem-cache measurements; they do not establish cold disk costs.
Setup clones of input plans/ASTs are outside stage timers, and returned-value
destruction is outside timers. Internal allocations, internal cleanup, and
consumption/destruction of stage inputs are inside. Tiny sub-microsecond numbers
are subject to timer noise. p95 uses the nearest-rank sample, so small sample
counts provide limited tail information.

Stages overlap by design: parsing includes tokenization; strict analysis includes
its validation; lowering includes analysis revalidation and IR verification;
LLVM emission includes backend verification. Stage medians should not be summed
as independent components. `frontend_to_ir` measures the combined path directly,
without the CLI entry adaptation, object generation, or linking.
`semantic_token_encoding` uses an empty role map to isolate token/position
encoding; semantic classification is included in `editor_analysis`.

LSP measurements run a separate release server over framed stdio, consume its
responses, and validate completion results. Standalone cases open 1, 5, or 20
documents with ten helpers each, then edit the first document. Separate cases
measure a cached completion request, a completion immediately following an edit,
ten queued edits followed by completion, and cached semantic tokens. Completion
uses an intentionally unfinished `std::io::pri` expression. Diagnostic timings
use valid programs and require empty diagnostics. A five-file explicit
`build.fib` project provides a comparison with standalone planning. Initialization
timing ends at the initialize response; std analysis starts when documents open.
Server teardown is outside the startup timer.

No statistical significance, allocation counts, sampled CPU profiles, or
guaranteed speedups are claimed. Source inspection explains plausible causes;
isolated stage timings constrain them but are not per-function CPU attribution.

## Results

The tables below report medians in milliseconds unless stated otherwise.
Raw CSV is retained for the [30-sample frontend/LSP baseline](benchmarks/2026-10-29-no-llvm.csv)
and [10-sample supplemental probes](benchmarks/2026-10-29-probes.csv).
Supplemental probes were run with:

```sh
FIB_BENCH_SAMPLES=10 FIB_BENCH_FILTER=collect_declarations,resolve_names,resolve_types,clone_,verify_ir,sample_,lsp_service,lsp_stdio/5_project_documents cargo bench --offline --no-default-features --bench performance
```

The [LLVM/backend CSV](benchmarks/2026-10-29-llvm.csv) contains 10 samples per case,
using LLVM 21 and the matching LLVM-enabled release compiler:

```sh
LLVM_SYS_211_PREFIX=/usr/lib/llvm/21 FIB_BENCH_SAMPLES=10 FIB_BENCH_FILTER=emit_llvm,write_object,cli_build_and_link cargo bench --offline --bench performance
```

### Compiler frontend and lowering (30 samples, no LLVM)

| Helpers | Std | Parse plan | Strict analysis | Collecting analysis | Revalidate analysis | Lower incl. validation | Frontend to IR |
| ---: | :---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 10 | off | 0.065 | 0.209 | 0.946 | 0.069 | 0.317 | 0.635 |
| 10 | on | 0.935 | 10.064 | 90.074 | 3.914 | 24.573 | 35.862 |
| 100 | off | 0.610 | 1.796 | 68.704 | 0.606 | 18.742 | 21.119 |
| 100 | on | 1.487 | 15.148 | 291.337 | 5.752 | 62.702 | 79.447 |
| 500 | off | 2.748 | 9.283 | 1703.764 | 3.285 | 875.120 | 883.068 |
| 500 | on | 3.616 | 40.072 | 2580.624 | 14.476 | 1108.118 | 1151.063 |

The no-std collecting-analysis p95 at 500 helpers was 1,912.951 ms; lowering p95
was 882.818 ms. The large difference between strict and collecting analysis is
present on valid source with no diagnostics to recover from. Tokenization alone
took 0.010, 0.111, and 0.534 ms at the three sizes. Preparing an unprepared std
plan took about 0.099 ms. Neither cost explains the collecting-analysis latency.

### LSP stdio (30 samples)

| Open standalone documents | Edit → diagnostics | Cached completion | Edit → completion | 10 edits → completion | Cached semantic tokens |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 96.022 | 0.020 | 76.040 | 152.383 | 0.018 |
| 5 | 486.190 | 0.029 | 462.879 | 930.229 | 0.025 |
| 20 | 1988.685 | 0.026 | 1968.368 | 3919.656 | 0.029 |

Initialization alone took 1.145 ms median. A thousand direct service completion
lookups on the short incomplete document took 0.322 ms total (approximately
0.322 µs each); real stdio includes scheduling, JSON encoding/decoding, and pipe
round trips. The one-document edit-to-completion p95 was 77.160 ms, and burst
p95 was 153.448 ms. Diagnostic and completion edit cases use different source:
the former is valid and the latter intentionally fails parsing near `std::io::pri`.
They should not be interpreted as the same analysis workload.
The 20-document burst p95 was 4,007.876 ms. This exposes a user-visible stall
even though all ten updates request the same incomplete source text.

The five-file explicit project measured 205.571 ms median and 207.254 ms p95
(10 samples), including build-file planning/interpretation. This project has
only five small functions, whereas the five standalone documents have 55 total;
it is an additional workload, not a controlled estimate of grouping speedup.

### Supplemental isolated probes (10 samples)

| Helpers, no std | Collect declarations | Resolve names | Resolve types | Clone analyzed ASTs | Clone IR | Verify final IR |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 10 | 0.003 | 0.025 | 0.109 | 0.008 | 0.003 | 0.020 |
| 100 | 0.019 | 0.247 | 0.914 | 0.085 | 0.033 | 0.309 |
| 500 | 0.092 | 1.229 | 4.997 | 0.301 | 0.110 | 4.275 |

At 500 helpers with std, resolving types took 23.813 ms, cloning IR took
0.169 ms, and verifying final IR took 6.700 ms. These are independent probes,
not directly additive pieces of the earlier full-stage medians. A single clone
of `SemanticTypes` was only 0.003 ms without std and 0.037 ms with std at 500
helpers; the collecting checkpoint also clones function ASTs/recipes and runs
many times. Type-table copying alone should not be blamed for the whole gap.

The real `generic_map` sample took 19.940 ms for strict analysis and 108.566 ms
for editor analysis. `process_entry` took 10.706 and 82.773 ms respectively.
These sample measurements stop at analysis, so they do not time process startup
wrapper synthesis or runtime map operations.

| Cached completion input | Direct lookup per request (ms) |
| --- | ---: |
| Short unfinished expression | 0.000321 |
| After 10 helpers | 0.024168 |
| After 100 helpers | 0.236697 |
| After 500 helpers (~50 KB) | 1.590661 |

The short-expression probe batches 1,000 requests; the others batch 100 requests.
Per-request figures divide each batch median by its request count. The larger
document probes append an incomplete expression after the generated source;
they do not include transport or reanalysis in their timers.

### LLVM and full CLI build (10 samples)

| Helpers | Std | Emit LLVM incl. verification | Write object from artifact | Fresh CLI compile + link |
| ---: | :---: | ---: | ---: | ---: |
| 10 | off | 0.177 | 0.892 | 28.989 |
| 10 | on | 4.561 | 20.143 | 96.093 |
| 100 | off | 1.427 | 4.828 | 57.273 |
| 100 | on | 6.446 | 25.569 | 146.497 |
| 500 | off | 12.351 | 22.090 | 976.516 |
| 500 | on | 19.890 | 42.405 | 1275.701 |

The 500-helper CLI build p95 was 1,019.282 ms without std and 1,290.701 ms with
std. CLI timings include process startup, source reads, entry validation, normal
compiler output captured through pipes, object emission, and host `cc` linking.
The application sources, object, and output executable live in a temporary
directory; input generation and writing source text happen outside the timer.
Object timings include reparsing the artifact's textual IR and writing a real
object file. These are warm repeated builds, with no output-cache shortcut.

For the large no-std case, the earlier 875.120 ms lowering median is much larger
than LLVM emission plus object generation. Fixing repeated middle-end work is a
better-supported first investigation than speculative LLVM instruction tuning.
For tiny inputs, process/linker and unused std work are comparatively significant.

## Optimization candidates

### 1. Reduce collecting-analysis rollback copying

**Measured:** Without std, 5× more helpers (100 → 500) produced about 24.8× more
collecting-analysis time, versus 5.2× for strict analysis. At 500 helpers,
collecting analysis was about 184× slower than strict analysis. This directly
affects the editor's analysis path.

**Source evidence:** `TypeContext::statement_collecting` in
[`statement_typing.rs`](../crates/fib-frontend/src/frontend/analyzer/statement_typing.rs) clones both
`TypeState` and `SemanticTypes` before checking each statement when recovery is
enabled, including successful statements and nested blocks. `TypeState` contains
declaration recipes and all function AST snapshots. Declaration collection also
clones the current statement and accumulated namespace for rollback in
[`analyzer.rs`](../crates/fib-frontend/src/frontend/analyzer.rs).

**Candidate:** Keep immutable recipes/function snapshots shared and use small
transaction logs or checkpoints for the state that a statement can mutate.
Avoid copying unrelated declarations/types on every successful statement. The
measurements and code support this as a strong candidate, but do not isolate
the time spent in each individual clone.
The inexpensive isolated semantic-type clone suggests that the large immutable
function/recipe snapshots in the checkpoint deserve particular attention.

**Required correctness constraints:** Failed statements must roll back their
annotations/state and keep independent diagnostics available. Preserve
unavailable-file handling, generic-instance recovery, name/type identities,
and the separation between collecting reports and strict lowering inputs.

### 2. Avoid repeated whole-program IR transactions and verification

**Measured:** Without std, 100 → 500 helpers increased lowering time from
18.742 to 875.120 ms (46.7×), while independent analysis revalidation increased
from 0.606 to 3.285 ms. At 500 helpers, lowering accounts for almost all the
883.068 ms frontend-to-IR time, so revalidation alone cannot explain it.

**Source evidence:** [`FunctionBuilder::new`](../crates/fib-ir/src/ir/builder.rs) clones the
entire accumulated `Ir` for each function. `finish` verifies the whole working
IR and commits it. [`lower_with_entry`](../crates/fib-lowering/src/lowering.rs) uses this builder
per function and verifies the whole IR again at the end. The verifier allocates
a predecessor vector sized to all program blocks for each defined function in
[`verify_ir`](../crates/fib-ir/src/ir/verify.rs). Repeating growing-program work at each function
creates substantial superlinear work even for small, independent function bodies.
The isolated final verification probe also grew superlinearly (0.309 → 4.275 ms
for 100 → 500 helpers). It is therefore worth measuring verifier workspace
allocation separately from the number of times verification is invoked.

**Candidate:** Use append-only checkpoints or per-function transactions and
function-local ownership/CFG verification, with one complete verification at the
public program boundary. Use function-local predecessor storage where possible.
Retain transactional rollback, global ownership/dominance checks, and strict
rejection of malformed IR; simply removing verification is not a safe substitute.

### 3. Share unchanged analysis work across LSP snapshots/documents

**Measured:** A cached completion on the tiny input took about 0.020 ms over
stdio, but completion immediately following an edit took 76.040 ms with one
document. Ten edits followed by completion took 152.383 ms despite requesting
only the final result. These numbers show analysis waiting time dominates this
particular completion case; they do not predict lookup cost on larger documents.
Five open standalone documents increased edit-to-diagnostics latency to
486.190 ms (5.1× the single-document result), while cached completion stayed at
0.029 ms. This supports the repeated-analysis explanation independently of the
completion lookup cost.
At 20 standalone documents, edit-to-diagnostics latency was 1,988.685 ms
(20.7× the one-document result); cached completion was still 0.026 ms.

**Source evidence:** [`analyze`](../src/bin/fib-lsp.rs) independently calls
`tooling::analyze_plan` for standalone documents. Each call prepares the plan,
reads/tokenizes/parses std, and runs collecting analysis of the whole plan.
`schedule` clears cached results on every edit and clones the full document
snapshot. The worker drains queued snapshots before starting a job but does not
cancel an already-running analysis. A burst can therefore wait for obsolete
work and then for current work.

**Candidate:** Cache parsed std sources and unchanged project inputs; analyze
shared packages once per snapshot; invalidate according to source/dependency
changes. Investigate cooperative cancellation at phase/file boundaries. Preserve
current-generation responses and diagnostics, sysroot manifest validation,
per-file visibility, stable plan IDs, and unsaved sibling std overlays. Parsed
source caching is a smaller initial step than reusing mutable semantic state.

### 4. Reduce duplicate validation and LLVM round trips after larger costs

**Source evidence:** Strict analysis calls `validate_analysis`, and lowering
calls it again; validation clones analyzed ASTs before checking annotations.
LLVM `emit_llvm` calls `verify_input`, then `emit_classified` calls it again in
[`llvm_lower.rs`](../crates/fib-backend/src/backend/lowering/llvm_lower.rs). `LlvmArtifact` stores
textual LLVM IR; `write_object` creates a context, reparses/verifies that text,
and creates another target machine. Target-machine creation initializes targets.
The CLI also clones parsed units before initial analysis and reanalyzes when a
process startup wrapper is needed in [`driver.rs`](../src/driver.rs).

**Candidate:** Preserve validation at public boundaries while avoiding redundant
checks inside an already-validated path. Consider an internal verified artifact
or emission session that keeps the LLVM module/target machine alive through
object output. Measure the wrapper-specific path before prioritizing changes to
process entry analysis. These are source-supported opportunities, not measured
speedup estimates.

### 5. Share completion tables and index source positions if lookup warrants it

**Measured:** Cached direct lookup grew from 0.000321 ms on the tiny expression
to 1.591 ms after 500 helpers, without transport or reanalysis. This supports
investigating the per-request source/token scan even though it is much smaller
than the multi-second analysis delays in the many-open-document cases.

**Source evidence:** [`apply_report`](../crates/fib-frontend/src/frontend/tooling.rs) constructs
namespace paths, completion items, and sorted tables independently for each
document, including std documents. `complete` constructs a `SourceIndex` and
scans token ranges on every request to exclude comments/literals; the final
prefix lookup uses a `BTreeMap` and clones matching items.

**Candidate:** Share immutable completion tables where the dependency/visibility
environment matches, and cache line/UTF-16 indices or locate the cursor token
directly. Keep per-request visibility, comment/string exclusion, and UTF-16
correctness. Do not assume completion lookup is the main source of edit latency:
requests following edits wait for the full current analysis generation.

### 6. Investigate unused standard-library code emission

**Measured:** At ten helpers, enabling std increased lowering from 0.317 to
24.573 ms, object generation from 0.892 to 20.143 ms, and the full CLI build
from 28.989 to 96.093 ms, even though the generated application never calls std.

**Source evidence:** [`lower_with_entry`](../crates/fib-lowering/src/lowering.rs) traverses all
analyzed units and lowers ordinary non-generic function bodies, including
unreferenced std functions. Source std is loaded before analysis by design.

**Candidate:** Keep full semantic validation but investigate reachability-based
body lowering/emission from the selected entry and externally required symbols.
Account for function pointers, global address constants, syscalls/intrinsics,
generic instances, and startup wrappers. This requires a sound reachability
contract; the numbers do not establish how much time pruning would save.

## Validation

All recorded benchmark cases completed successfully. Compiler stage probes
require successful parsing/strict analysis/lowering; collecting/editor cases
reject unexpected errors; stdio completion cases require a `print` suggestion.
This is benchmark fixture validation, not a new claim that every compiler test
or interactive editor scenario passed.

Formatting, whitespace checks, and strict Clippy for all targets passed both
without default features and with LLVM 21. The benchmark target also passed a
normal `cargo test --offline --no-default-features --bench performance`
invocation, confirming measurements are skipped outside benchmark mode.
