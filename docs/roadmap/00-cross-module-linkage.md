# 00 — Canonical Cross-Module Linkage Names

**Depends on:** nothing. **Blocks:** [06 — Function Overloading](06-function-overloading.md).
**Kind:** bug fix. Belongs to Phase 3 "Declaration Identity and Linkage".

## Problem

Calls across modules to non-`extern` functions do not link.

`src/frontend/analyze/expressions.rs:849-855` mangles a qualified callee to
`{alias}__{member}`, but the defining module emits the definition under its bare
source name (`src/backend/lowering/llvm_lower.rs:34`, fed by
`src/driver.rs:814-815`). The call then falls into the auto-declare path in
`src/backend/lowering/expressions.rs:924-942` and fails at link time.

Reproduce with two files and `--emit=typed`: the call site emits
`helper__add_one` while the definition is emitted as `add_one`.

The bug is invisible today because every cross-module callee in `samples/` and
`std/` is `extern`, and `extern` callees are deliberately left unmangled.

A second defect hides behind the same code: the call site mangles with the
**import alias**, so two spellings of one module produce two different symbols.

## Design

One canonical symbol per declaration, computed by the **defining** module and
stored on the declaration. Call sites read it; they never recompute it.

- Add `TypedFunction::symbol: String` next to `name`. `name` stays the source
  name and remains what diagnostics, tests, and the LSP show.
- Compute it through a single `linkage_name(module_id, name, ...)` helper so
  Phase 3 can swap the module-identity half without touching anything else.
- `extern` functions keep their declared C symbol. `main` in the entry unit
  keeps `main`.
- `analyze()` does not currently know which module it is analyzing
  (`src/driver.rs:803` wraps the result in a `TypedModule` only afterwards).
  Add `analyze_in_module(ast, resolved, module_path)` and keep `analyze` as a
  wrapper passing `&[]`, so existing tests compile unchanged.

## Work

- [ ] Add `TypedFunction::symbol` and populate it at the three construction
      sites (`analyze/functions.rs:99`, `analyze/functions.rs:251`,
      `analyze/generics.rs:348`).
- [ ] Add `analyze_in_module`; pass the module path from `src/driver.rs:803`
      and `&[]` from `analyze_entry`.
- [ ] Add `linkage_name` behind a `ModuleId`-shaped parameter.
- [ ] Emit definitions under `symbol` in `backend/lowering/llvm_lower.rs:34`
      and `ir/mod.rs:306`.
- [ ] Resolve qualified call sites to the callee's `symbol`; delete the
      alias-based `format!` at `analyze/expressions.rs:853` and its duplicate at
      `backend/lowering/expressions.rs:1302`.
- [ ] Key `decl_key` (`src/driver.rs:841-846`) on `symbol` for functions.

## Tests

- [ ] Driver test: a non-`extern` function called across a module boundary
      compiles, links, and runs.
- [ ] The same function reached through two different import aliases resolves to
      one symbol and is emitted once.
- [ ] `main` is emitted as `main`; an `extern` keeps its declared C name.
- [ ] A diamond import still emits each declaration exactly once.

## Risks

- Every non-`extern`, non-`main` symbol is renamed. Any test asserting on IR
  text or on `decl_key` strings breaks — sweep for `fn:` and `__` first.
  `src/driver.rs:1430` asserts `decl_key(d) == "fn:shared"` and must change.
- A local `fn helper` and an imported `mod::helper` currently collide on
  `decl_key`, so the local one silently replaces the imported one — which
  breaks that module's internal calls to its own `helper`. After this change
  both are emitted. That is correct, and it changes `.ll` output.
