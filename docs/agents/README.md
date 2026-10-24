# Compiler implementation guide

Reviewed against this checkout on 2026-10-19. Source and tests take precedence over
historical comments, samples, and language proposals. Semantic reconstruction
updates were made on 2026-10-21; final review fixes passed the blocking pre-commit
checks, including LLVM-enabled library and executable acceptance tests.

## Actual status

Working components are the tokenizer, parser, AST construction, lexical scope
utilities, diagnostic rendering, and raw IR data/access/formatting utilities.
The name pass validates named units, annotates declarations, and resolves lexical
names, file paths, visibility, and compile-time aliases. Semantic typing/member
lookup now execute and validate their annotations. Explicit metatype
specialization now rechecks concrete bodies, retaining symbolic template checks.
Bounded type computation requires explicit `comp fn` eligibility. Independent
binding/element mutability, `never` bottom coercion, definite initialization and
structured control-flow checks run; see [typing](../type-resolution.md). Variadics
are removed. Numeric literals use arbitrary-precision integer magnitudes and direct decimal-to-IEEE
rounding; constant evaluation implements fixed-width wrapping and exact IEEE
operations. Single-file compilation supports fixed-width signed/unsigned integers
through 128 bits, float16/32/64/128 and bool through locals, arithmetic,
bitwise/shifts, explicit casts, short-circuit logic, helpers/calls, conditionals,
while loops, break/continue and returns. Typed numeric values require explicit
conversions. Native helpers use a limited direct Fib fastcc ABI; main keeps the
C int32 entry contract. Struct values, partial initialization, stack storage,
field/pointer mutation and internal struct/pointer calls work. Ordinary dereference
traps on null; `memory::unchecked_deref(pointer)` returns an unchecked place.
Defer, runtime globals and broader runtime representations remain unfinished. See
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
  -> object -> cc -> executable (scalar/control-flow subset)

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

## Map

- [Repository and commands](repository.md)
- [Source ownership](src/README.md)
- [Tokenizer, parser, AST, and analysis](src/frontend/README.md)
- [SSA IR](src/ir/README.md)
- [LLVM backend](src/backend/README.md)
- [Tests and validation](tests/README.md)
- [Samples](samples/README.md) and [standard library](std/README.md)
- [CI](.github/workflows/ci.md)
- [Open design boundaries](language-design-questions.md)
- [Documentation overview and language-design proposals](../README.md)
- [Verified parsing snapshot](../parsing/README.md), not a language specification
- [Analysis issues on GitHub](issues/README.md)

Legacy page names are retained as explicit redirects/status pages so existing
links remain useful. No missing roadmap is implied by this index.

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
