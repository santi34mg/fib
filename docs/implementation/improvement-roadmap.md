# Language and Compiler Improvement Roadmap

This roadmap consolidates the 2026-09-23 parallel exploration of the frontend,
driver/module system, middle/backend, tests, tooling, samples, and standard
library. Priorities reflect correctness and safety, not implementation size.

## P0: Restore Sound Stage Boundaries

### Canonical module and symbol identity

**Problem:** qualified calls are analyzed using alias-derived names such as
`alias__function`, while imported function declarations retain bare names.
Backend flattening in `dedupe_declarations` also deduplicates by declaration
kind and unqualified name. Two modules can therefore collide, and ordinary Fib
imports can call a symbol that is never defined.

**Direction:** assign every declaration a canonical identity such as
`(ModuleId, SymbolId)` and one final linkage name. Carry it through exports,
typed calls, typed declarations, flat IR, and both LLVM routes. Extern symbols
must retain their declared foreign name.

**Proof required:** link-and-run tests for direct imports, aliases, selective
imports, diamond imports, and two modules exporting the same local name.

### Complete semantic validation before lowering

The analyzer must establish these invariants before any backend runs:

- conditions and logical operands are boolean;
- integer-only, floating, pointer, and equality operators have legal operands;
- casts match an explicit source/target legality matrix;
- `null` coerces only to pointer-like types and is not treated as bottom;
- every assignment form checks lvalue mutability and target/value type;
- every local read is definitely initialized;
- every reachable path in a non-void function returns;
- payload enum variants cannot be constructed without payloads;
- generic calls have exact compile-time and runtime arity;
- all runtime types resolve and have finite layout.

Add negative analyzer tests for every rule. Backend rejection is not an
acceptable substitute for a source-level semantic error.

### Verify intermediate and generated code

Add a structural flat-IR verifier for labels, terminators, temporary
definitions/dominance, operands, and function signatures. Call LLVM module
verification in both LLVM routes before returning textual IR. Verification
failure is an internal compiler error and must never trigger silent fallback.

### Repair lowering correctness

- Replace flat-IR short-circuit temporary reassignment with PHIs, block
  parameters, or addressable temporary storage.
- Reject unsupported pointer operations before the IR consumer performs an
  unchecked Inkwell value conversion.
- Track direct-backend locals by resolved binding identity or with a scoped
  storage map; source-name maps leak shadowed branch/loop bindings.
- Predeclare every function before lowering bodies in both routes.
- Stop lowering ordinary instructions after a terminator.
- Define stack-backed slice escape rules or add a lifetime-safe representation.
- Compute tagged-union size and alignment from LLVM target data.

### Quarantine unsafe standard-library APIs

Mark `std/` experimental and classify each module as usable, stub, unsafe, or
platform-specific. Remove success-returning no-op APIs and APIs named atomic
that perform ordinary loads/stores. Repair buffer growth, opaque C layouts, and
FFI signatures before advertising those modules.

## P1: Reliability and Determinism

### Make frontend emit stages real

`EmitKind::Lex` and `EmitKind::Parse` currently call the complete frontend,
including import resolution and analysis. Split loading/lexing, parsing, and
resolution/analysis so a stage emit cannot fail because of a later stage.

### Harden parsing and type/name resolution

- Centralize delimited-list parsing and reject EOF before every closing token.
- Decide whether commas and semicolons are mandatory, then align parser and docs.
- Detect alias cycles and infinitely sized recursive value types.
- Validate qualified and unqualified type names during analysis.
- Diagnose duplicate declarations, aliases, fields, variants, parameters, and
  local bindings instead of silently replacing symbols.
- Restrict `defer` and loop header statements until their full semantics are
  specified.

### Make graph and output order deterministic

Do not derive semantic or emitted declaration order from `HashMap::values()`.
Preserve source/topological order or sort by canonical identity. Add repeated
build tests for stable typed output and LLVM IR, excluding intentional path
metadata.

### Preserve source information

Use source IDs and byte ranges rather than point-only line/column spans. Keep
module source in resolution state, and preserve spans through typed AST and IR
so imported-module and backend diagnostics identify the actual file and range.

### Define `defer` and evaluation semantics

The two lowering routes differ on normal nested-block fallthrough. Both run
deferred work before evaluating a return expression. Specify function-scope or
block-scope behavior, LIFO order, interaction with return/break/continue, and
return-expression evaluation timing, then enforce one model in analysis.

## P2: ABI, API, and Tooling

- Set target triple and data layout; derive pointer, `usize`, slice, aggregate,
  and tagged-union layout from target data.
- Implement C default argument promotions for variadic extern calls.
- Add native linker options for library paths, libraries, arguments, target,
  and sysroot. Preserve a configured `$CC` command rather than its first word.
- Unify `compile_project` and `check_project` under one public diagnostic type.
- Reduce accidental public API exposure from `pub mod` declarations.
- Categorize lexical, name, type, toolchain, and configuration diagnostics
  accurately and preserve underlying error sources.
- Add machine-readable diagnostics after source ranges and stable categories
  are established.
- Decode string escapes exactly once and add integer-literal range checking.

## P3: Coverage, Performance, and Ergonomics

- Auto-discover samples and require each to declare exact-output, compile-only,
  or expected-failure policy.
- Convert `tests/probe_tmp.rs` into asserting diagnostics regressions.
- Add forced-path differential tests for constructs supported by both lowering
  routes.
- Frontend-check every std module and run a safe representative subset.
- Add lexer/parser no-panic fuzzing and randomized module/type-cycle tests.
- Hoist fixed-size allocas to entry blocks and replace aggregate spill/reload
  sequences with LLVM aggregate SSA operations where useful.
- Borrow symbol tables during type mapping instead of cloning them.
- Remove confirmed unused dependencies and add package metadata.
- Add a package manifest, installed std discovery, `fibc run`, formatter, and
  editor tooling only after semantic soundness is stable.

## Language Design Decisions Needed

Implementation should pause for an explicit decision on these points:

1. Numeric promotion, narrowing, literal range, overflow, and divide-by-zero rules.
2. `null` and `@never` semantics.
3. Mandatory versus optional separators and semicolons.
4. Function-level versus lexical-block `defer`.
5. String ownership, embedded NUL behavior, and allocation failure.
6. Module visibility, re-export, ambiguity, and declaration collision policy.
7. Slice lifetime and escape model.
8. Float NaN comparison behavior.
9. Reserved syntax such as `union`, function values, and module constants.

Record decisions in maintained language pages and tests before extending the
backend.
