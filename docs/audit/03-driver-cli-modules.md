# Driver, CLI, and Module Resolution

> Note (2026-09-18): addressed items removed. This file has NO remaining
> points. Removed: `compile()` stage split (+ `--emit`, `FrontendResponse`
> wiring), configurable `--output/--emit-llvm/--cc/-O` + `CC` env +
> tempfile + distinct stdout/stderr, feature-gating (`--check` without
> LLVM, no silent no-op), parser error preservation via
> `DriverError::Parse(#[from])`, module-resolution duplication (see below).

## Current state (verified 2026-09-18)

- `compile()` is staged (`validate_path`, `read_source`,
  `lex/parse_source`, `resolve_imports`, `analyze_entry`, `run_frontend`,
  `lower_to_llvm_ir`, `emit_llvm_ir`, `link_llvm_ir`, `compile`
  orchestration).
- §1(a) Single load path: `load_module` (`driver.rs:525`) is the
  import resolve-read-parse entry; the entry file goes through
  `load_entry` (`:537`), and both funnel through the shared
  `lex_source`/`parse_source` core (documented at `:520-545`) — exactly
  one lex+parse path.
- §1(b) Canonical keys + path interning: `canonicalize_module_path`
  (`:558`) + `path_intern` inside `resolve_imports` (`:604`) map one
  canonical `PathBuf` per distinct file; alias-spelled imports of the
  same file reuse the cached `TypedModule` (no re-read/re-parse/re-analyze),
  and alias cycles still report `CircularImport`. The two-way search
  (full path + drop-first-segment fallback, `module_candidate_paths` `:415`)
  is retained and documented as load-bearing for `-I <repo>/std`.
- §1(c) Lowering-side diamond-import hacks are REMOVED: `llvm_lower.rs`
  no longer carries the `count_basic_blocks() > 0 => continue` guard and
  `ir_lower.rs:80-82` has no duplicate-skip — both rely on
  `dedupe_declarations` (`:749`). Proven by
  `resolve_imports_diamond_is_deduped` (`:1229`) and the full-pipeline
  compile-and-run diamond proof (`:1320`).
- Cycle detection is `HashSet<Vec<String>>` + `resolving_stack` with
  `CircularImport{stack}` (joined `" -> "`), tested.

No remaining points. The short-term slice is complete; a real module
system (visibility, separate compilation, incremental cache) is a
multi-week project deliberately out of scope — revisit only on demand.