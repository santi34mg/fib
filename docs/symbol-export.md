# Public symbol JSON export

Export the public surface of a single source or every source explicitly selected
by a build file:

```sh
fibc export-symbols --single-file api.fib --sysroot library > symbols.json
fibc export-symbols --build-file build.fib --no-std > symbols.json
```

The command prepares and strictly analyzes the project. It needs no `main`
function, LLVM, object generation or linking. Build files use the existing narrow
interpreter. Input errors fail the command without writing partial JSON;
warnings and errors go to stderr. Std selection follows the build command's
`--sysroot`, `FIB_SYSROOT`, installed-source and `--no-std` rules.

Output is one object containing `schema_version: 1` and a `symbols` array, sorted
by package identity, normalized module path and source declaration name. All
source files in the prepared plan are included, including selected dependency
and std sources. Implicit primitive prelude declarations and synthetic modules
are excluded. Discovery is limited to files in the plan.

Only unrestricted `pub` top-level names are exported. Private declarations,
`pub(dir)`, `pub(root)`, locals, fields and enum variants are excluded. Each
binding in a multi-binding declaration is considered independently. Public
aliases appear under their own names even when their targets are private.

Each symbol contains:

- `package`, `module`, `name`: persistent package identity, normalized module
  path and original declaration spelling; no build-local IDs or linker names.
- `kind`: `function`, `type`, `value` or `alias`. Values include constants and
  mutable globals; aliases retain their own kind.
- `source`: original `path` and one-based declaration-start `line` and `column`
  in Unicode scalar columns. Paths reflect the prepared build plan.
- `target`, for aliases: resolved target's `package`, `module` and
  optional `name` (omitted for namespace targets). Primitive prelude targets
  use package `compiler` and module `prelude`.
  Other synthetic compiler targets use their compiler namespace as `module`.
- `signature`, for function declarations: ordered parameter `name`/`type`
  pairs, `return_type`, `compile_time`, `extern` and `calling_convention`
  (`Fib` or `C`). Type strings use the compiler's human-readable syntax display;
  they are not a machine-readable semantic type graph or ABI description.
- `type`, when the binding has a canonical primitive type: its canonical name,
  including for inferred values. Nonprimitive binding types omit this field.

An empty public surface produces `"symbols": []`. Shell redirection selects the
destination; avoid redirecting onto an input source file.
