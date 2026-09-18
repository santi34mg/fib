# Driver, CLI, and Module Resolution

> Note (2026-09-18): addressed items removed. This file now lists only the
> partially addressed point. Removed: `compile()` stage split (+ `--emit`,
> `FrontendResponse` wiring), configurable `--output/--emit-llvm/--cc/-O` +
> `CC` env + tempfile + distinct stdout/stderr, feature-gating (`--check`
> without LLVM, no silent no-op), parser error preservation via
> `DriverError::Parse(#[from])`.

## Current state (verified 2026-09-18)

- `compile()` is staged (`validate_path`, `read_source`, `lex/parse_source`, `resolve_imports:469`, `analyze_entry:557`, `run_frontend:595`, `lower_to_llvm_ir:631`, `emit_llvm_ir:695`, `link_llvm_ir:737`, `compile:813` orchestration).
- Module loading is half-unified: `load_module_source() -> (PathBuf, String)` (`driver.rs:406`) + `parse_module_ast` (`:459`) are reused for transitive imports (`:509-510`); old `FIXME` and `resolving: Vec<Vec<String>>` are gone. Cycle detection is `HashSet<Vec<String>> resolving_set` + `resolving_stack` (`:474-475`) with `CircularImport{stack}` (`:162`, joined `" -> "` at `:221-223`, tested at `:1087`).
- Driver dedupes by symbol (`decl_key fn:/type:/const:` at `:564`, `dedupe_declarations:579`, used at `:844-845`) — but the lowering-side duplicate guard remains: `llvm_lower.rs:80-82` (`count_basic_blocks() > 0 => continue`) and `ir_lower.rs:80-84` (`return Ok(()) // duplicate via diamond import`).
- Keys are still `Vec<String>` (`:473` `HashMap<Vec<String>, TypedModule>`); no `canonicalize()`, no path interning, no `Ast`/error cache. Two-way search (full path + drop-first-segment fallback, `:415-440`) is retained.

## Remaining points

### 1. Module resolution duplication [PARTIALLY ADDRESSED]
Done: shared `load_module_source`/`parse_module_ast` for imports, `HashSet` cycle detection with import stack, driver-side `dedupe_declarations`.

Remaining:
- a) Single `load_module(path, search_roots) -> Result<(PathBuf, String, Ast)>` reused for the entry file too (entry still does `read_source + lex + parse` at `:598-607` instead of going through it).
- b) Canonicalized keys + file-path interning; cache `Ast`/`TypedModule` by resolved path (keys are still bare `Vec<String>`; no parse-error cache with file context). Decide whether to keep the two-way search or document it.
- c) Remove the lowering-side diamond-import hacks (`llvm_lower.rs:80-82`, `ir_lower.rs:80-84`) once driver dedupe fully covers it — dedupe must be proven by a diamond-import test first.

Trade-offs:
- **Correctness vs. velocity.** A real module system (visibility, separate compilation, incremental cache) is a multi-week project. Items (a)-(c) above are the short-term slice that removes the duplication without redesigning modules.
