# Compiler implementation guide

The [language specification](../language-specification.md) is implemented for prelude names, casts, shallow readonly views,
warned pointer field access and exhaustive switches. Concrete generic functions,
multiple source files and fixed-array values execute. Package-rooted lookup, scoped
visibility, installed std sources and Linux x86-64 syscalls execute. Recursive source
discovery remains deferred. Checked runtime indexing/slicing and constant-initialized
globals execute; value-returning `comp fn` execution remains explicit scaffolding.

Status reviewed against source and tests on 2026-10-28. Source and tests take
precedence over historical comments, samples and language proposals. Dated
validation results are recorded in the [test guide](tests/README.md). The source std
allocator now executes through Linux `sbrk`, with aligned blocks, reuse, splitting
and coalescing. Earlier suite records remain dated below.

## Actual status

The source std I/O module implements literal output and tagged-argument `{}`
formatting, with escaped braces, format validation before writes, full-range
64-bit integer conversion and write loops handling short writes and EINTR.

The source std memory module implements byte-slice `memset` and `memcpy` with
checked loops. Copy accepts readonly sources and checks destination capacity
before writing; callers must provide nonoverlapping regions.

Working components are the tokenizer, parser, AST construction, lexical scope
utilities, diagnostic rendering, and raw IR data/access/formatting utilities.
The name pass validates named units, annotates declarations, and resolves lexical
names, file paths, visibility, and compile-time aliases. Semantic typing/member
lookup now execute and validate their annotations. Explicit metatype
specialization now rechecks concrete bodies, retaining symbolic template checks.
Bounded type computation requires explicit `comp fn` eligibility. Metatype
parameters and type values provide generic specialization. Independent binding/element mutability, `never` bottom coercion, definite initialization and
structured control-flow checks run; see [typing](../type-resolution.md). Variadics
are removed. Numeric literals use arbitrary-precision integer magnitudes and direct decimal-to-IEEE
rounding; constant evaluation implements fixed-width wrapping and exact IEEE
operations. Compilation of explicitly planned files supports fixed-width signed/unsigned integers
through 128 bits, float16/32/64/128 and bool through locals, arithmetic,
bitwise/shifts, explicit casts, short-circuit logic, helpers/calls, conditionals,
while loops, break/continue and returns. Typed numeric values require explicit
conversions. Native helpers use a limited direct Fib fastcc ABI; main keeps the
C int32 entry contract. Struct values, partial initialization, stack storage,
field/pointer mutation and internal struct/pointer calls work. Ordinary dereference
traps on null; `std::memory::unchecked_deref(pointer)` returns an unchecked place.
`core::sizeof(T)` returns `usize` byte sizes from LLVM's
selected target allocation layout, including padding. Concrete generic arguments
work. Values, void, never and metatypes are rejected. This target-dependent
operation is not yet available to frontend constant evaluation (including array
lengths, `comp` values and global initializers).
Enum constructors, payload copies, switches, storage and internal calls also execute; see [enum semantics](../enum-semantics.md). Defer statements and blocks execute in reverse order on lexical scope exits, including return, break and continue. Checked array/slice projections and constant-initialized globals also execute. Broader runtime representations remain unfinished. See
[scalar semantics](../scalar-semantics.md) for numeric rules and
[struct/storage semantics](../struct-storage-semantics.md) for memory rules.
`build.fib` has an
initial synthetic-module build API and shared-lowering/interpreter subset for
literal `build_api::{add_source, set_entry, set_output}` calls; deterministic project-plan finalization now validates its initial path/name/entry/output
contract; general interpretation remains unfinished.
See [driver](src/driver.md), [IR](src/ir/README.md), [backend](src/backend/README.md)
and [tests](tests/README.md) for exact limits.

```text
build --single-file PATH -> BuildPlan -> parse_files -> Analyzer
  -> AnalyzedProject -> ir::lowering::lower -> verified Ir -> verified LLVM
  -> object -> cc -> executable (scalar/struct/enum/pointer/control-flow subset)

build.fib + synthetic build_api -> Analyzer -> ir::lowering::lower -> verified IR
  -> limited IR interpreter -> PlanBuilder state (partial)
  -> validated project BuildPlan (narrow literal-operation subset)
```

The [future build-file plan](../roadmap/08-interpreted-build-files.md) fixes IR
interpretation—not native artifact loading—as the intended execution route.

There is no separate typed AST, direct AST-to-LLVM fallback, recursive import
loader, `compile_project` or `check_project`. `tests/executable.rs` checks real
compile/link/run behavior. Public architecture records alone do not certify
successful analysis, verification or support by the emitter.

Enum variants use `Name: Type` for one direct payload value. Explicit struct
payloads take one struct argument. Omitted, void and recursively unit-only struct
payloads normalize to bare variant values; calls on them are rejected. Optional
unit switch bindings have type void. Declared payload metadata is retained for
generic substitution and annotation proofs, while shared normalization determines
constructor, binding and storage behavior. Runtime unit values use empty aggregates
without changing resultless void returns or opaque `*void`.

## Language server

`src/bin/fib-lsp.rs` is a native stdio server sharing compiler tokens, parsing,
analysis and build planning through `frontend::tooling` and `frontend::source`.
`Analyzer::analyze_collecting` returns an `AnalysisReport` with partial semantic
information and independent diagnostics; it is never a lowering input.
`Analyzer::analyze_available` explicitly tracks files unavailable after parse/IO
errors and suppresses dependent checks. Strict `Analyzer::analyze` retains its
validated-project contract. Identifier annotation locations support highlighting
without a separate language grammar. See [LSP setup](../../lsp/README.md).

Directly opened std files use their owning sysroot manifest and reuse the open
document's ID in the std package. This preserves `root` and compiler intrinsic
identities, avoids duplicate application/library copies, and shares unsaved std
sibling overlays even when the running server uses another installed source tree.

## Specification implementation gaps

Primitive names resolve through protected synthetic prelude declarations. Direct
same-family literal cast context, explicit numeric/bool casts, shallow readonly
views, warned one-layer pointer field access and exhaustive switches are implemented.
Concrete rechecked generic instances and calls across explicitly planned files
lower to verified IR and LLVM. Fixed arrays support literals, inline storage,
whole-value copies/assignment, calls/returns and `.len`; multi-binding locals
initialize only after all RHS expressions evaluate.

`BuildPlan::packages` records explicit package IDs, source roots, dependency
maps and file membership. Scoped visibility uses package/directory regions;
public aliases preserve target identity without exposing the original path.
Existing single-file/flat-plan callers use the compatibility adapter.

Package-qualified lookup uses each source package's dependency environment and
package-relative file paths, with ASCII case/hyphen normalization and collision
checks. Flat-plan callers retain their compatibility path. Installed std is added
before parsing; see [standard library](../standard-library.md).
Still deferred: Unicode namespace normalization, public package/discovery build-API
signatures and discovery/deduplication integration, ownership/lifetime enforcement,
and broader compiler namespace spelling.
General build interpretation, value-returning compile-time function execution and broader ABI remain
unfinished. These are implementation boundaries, not amendments to the specification.

Globals require explicit constant initial data, including aggregate/enum values,
addresses and slices into global storage. No runtime startup initializer or `static`
keyword is needed. Ordinary initializer calls and runtime binding reads are rejected.
Value-returning `comp fn` definitions are type/flow checked; evaluation requests
report "not implemented". Existing bounded type-returning computation is preserved.
Slices borrow original storage without extending its lifetime. Dynamic array reads
and slicing require initialized backing arrays; bounds guards precede offset casts.
## Map

- [Repository and commands](repository.md)
- [Source ownership](src/README.md)
- [Tokenizer, parser, AST, and analysis](src/frontend/README.md)
- [SSA IR](src/ir/README.md)
- [LLVM backend](src/backend/README.md)
- [Tests and validation](tests/README.md)
- [Samples](samples/README.md)
- [CI](.github/workflows/ci.md)
- [Open design boundaries](language-design-questions.md)
- [Documentation overview and language-design proposals](../README.md)
- [Verified parsing snapshot](../parsing/README.md), not a language specification
- [Analysis issues on GitHub](issues/README.md)

## Maintenance rules

Distinguish working behavior, raw representation contracts, and intended future
passes. Do not turn a stub into successful empty output to satisfy tests. Preserve
source spellings/spans and stable identity through future passes. Add positive
and negative tests for implemented invariants. The phased name-resolution suite
covers names and identities. Separate type-resolution acceptance tests cover types,
expressions and members; all six former pending completion cases now pass. Update these pages with
subsystem changes; avoid line-number links and unverified runtime claims. Keep observed syntax in `docs/parsing/`;
parser limitations are not design decisions. Report analysis gaps as actionable
GitHub issues rather than embedding implementation backlogs in user-facing
language documentation. Keep only GitHub links in the issue index, not local
copies of report bodies.
