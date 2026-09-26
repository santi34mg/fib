# `src/driver.rs`

## Responsibility

The driver owns:

- `EmitKind`, `CompilationOptions`, `CompileOutput`, and `DriverError`;
- path validation and source reading;
- tokenization and parsing entry/module sources;
- recursive import discovery, cycle detection, and canonical-path interning;
- entry/module semantic analysis;
- imported declaration flattening and deduplication;
- lowering-route selection;
- output path selection, LLVM file emission, and clang invocation.

## Frontend Flow

`run_frontend` validates and reads the entry source, lexes and parses it,
resolves all transitive imports, analyzes modules dependency-first, analyzes the
entry module, and returns tokens, AST, typed program, resolved modules, source,
and filename.

Important current behavior: all frontend emit kinds call this cumulative
function. `Lex` and `Parse` do not stop before later stages.

## Import Resolution

Search roots are the entry directory followed by user include paths. For each
root, `module_candidate_paths` tries the full logical path and then the
drop-first-segment fallback. Preserve this behavior because `import std::libc`
with `-I=std` depends on it.

`resolve_imports` performs depth-first resolution and tracks both logical paths
and canonical physical paths. This detects normal cycles and alias-spelled
cycles, and reuses one analyzed physical module under multiple logical aliases.

Current module representation exports every global symbol. There is no explicit
visibility model. Selectively imported symbols are inserted into the importing
global scope and may effectively be re-exported.

## Backend Flow

For LLVM or binary output the driver combines imported and entry declarations,
calls `dedupe_declarations`, then calls `lower_to_llvm_ir`. That function tries
flat IR import and flat IR to LLVM first. Any error causes silent retry through
the direct typed-AST LLVM path.

The selected route is not exposed in `CompileOutput` or diagnostics.

## Linking

Compiler candidates are explicit `--cc`, the first whitespace-delimited word of
`$CC`, `clang-17`, then `clang`. LLVM text is compiled and linked in one command.
Optimization is forwarded as `-O<level>`. Temporary IR is removed after a
successful normal binary build unless retention was requested.

## Invariants Worth Preserving

- entry directory precedes include roots;
- drop-first-segment import fallback remains ordered per root;
- canonical path interning deduplicates physical modules;
- import resolution unwinds in-progress cycle state on failure;
- entry declarations intentionally follow imports during current deduplication;
- frontend-only operation remains available without LLVM.

## Known Risks

- Imported non-extern call names and definition names are inconsistent.
- `dedupe_declarations` uses kind plus unqualified name and silently keeps the
  last declaration, losing module identity.
- imported declaration collection can depend on `HashMap` iteration order.
- the typed public result differs from the declaration set actually lowered.
- silent fallback masks flat-IR defects and clones/double-processes programs.
- module analysis diagnostics discard path/source context.
- import-read error selection can report not-found instead of a later concrete
  I/O error.
- output paths can conflict with each other or the input and writes are not atomic.
- `$CC` arguments are discarded; native library/search/target flags are absent.
- link messages refer to clang even when another tool was selected.

## Test Guidance

Driver unit tests cover root ordering, fallback paths, cycles, canonical reuse,
diamond imports, path policy, and output precedence. Module correctness changes
must additionally link and execute real user-defined imported functions; LLVM
text substring checks do not prove symbol resolution.
