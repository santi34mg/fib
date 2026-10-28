# fib-lsp

The native Rust language server uses Fib's compiler tokenizer, parser and analyzer
for semantic highlighting and diagnostics. Rebuild it after compiler updates;
there is no separate editor grammar or JavaScript language implementation.

Diagnostics cover declaration collection, aliases/name resolution, types and
members, generic specialization, initialization, control flow, and compiler
warnings. Independent failures are collected across files and statements.
Checks that depend on failed declarations are deferred to avoid cascades.
Parsing currently reports the first syntax error in each file, while other
available files continue through analysis. Hover, completion and navigation
are not implemented.

## Install and run

From the compiler checkout, using the repository's Rust toolchain:

```sh
cargo install --path . --no-default-features --bin fib-lsp
fib-lsp
```

The server speaks LSP over stdin/stdout. No LLVM or Node runtime is needed.
For development:

```sh
cargo build --no-default-features --bin fib-lsp
./target/debug/fib-lsp
```

The former npm package and `lsp/src/server.js` launcher have been removed.
Replace editor commands pointing at Node or that script with `fib-lsp` or the
absolute path to the native executable.

## Projects and editing

The server finds the nearest ancestor `build.fib`, stopping at the containing
workspace root when supplied by the editor. It uses the compiler's existing
build interpreter to select the exact source files. Its supported build-language
subset and file/path validation are the same as command-line compilation.
Build-file analysis includes the compiler-provided `build_api` namespace.

Open documents, including `build.fib`, override disk contents without writing
source files. Unopened dependencies are read from disk. Standalone files and
files outside a valid project plan are checked independently and need no `main`.
An invalid build file receives diagnostics; other open files retain highlighting
until a valid plan can be produced. Planning does not emit objects or executables.

Full-document edits trigger background analysis. Results from obsolete snapshots
are discarded. Closing documents removes their overlays; watched `.fib` changes
reload disk dependencies and project membership. Clients that support dynamic
watch registration are registered automatically; VS Code also supplies a watcher.
Clients without file watching refresh dependencies on the next document edit.
Diagnostics use UTF-16 positions and include document versions for open files.

## Editors

### Neovim 0.11+

```lua
vim.filetype.add({ extension = { fib = "fib" } })
vim.lsp.config("fib_lsp", {
  cmd = { "fib-lsp" },
  filetypes = { "fib" },
  root_markers = { "build.fib", ".git" },
})
vim.lsp.enable("fib_lsp")
```

### Helix

Add to `~/.config/helix/languages.toml`:

```toml
[language-server.fib-lsp]
command = "fib-lsp"

[[language]]
name = "fib"
scope = "source.fib"
file-types = ["fib"]
roots = ["build.fib", ".git"]
language-servers = ["fib-lsp"]
```

### Zed

The independent Rust/Wasm extension in `editors/zed` registers `.fib` files
and launches `fib-lsp` from the worktree PATH. It does not bundle the server.

1. Install `fib-lsp` as above and ensure Zed inherits its PATH.
2. Prepare the extension toolchain:

   ```sh
   rustup toolchain install stable
   rustup target add --toolchain stable wasm32-wasip2
   ```

3. Run **zed: install dev extension** and select `lsp/editors/zed`.
4. Enable highlighting:

   ```json
   { "languages": { "Fib": { "semantic_tokens": "full" } } }
   ```

For a checkout executable, set:

```json
{
  "lsp": {
    "fib-lsp": {
      "binary": {
        "path": "/absolute/path/to/fib/target/debug/fib-lsp",
        "arguments": []
      }
    }
  }
}
```

The extension preserves binary arguments and environment overrides. Its local
`rust-toolchain.toml` uses stable with `wasm32-wasip2`, independently of the
compiler toolchain. Fully restart Zed after PATH/toolchain changes. Use
**zed: open log** for underlying extension build or server startup errors.
No Tree-sitter grammar is bundled, so syntax coloring relies on semantic tokens;
structural indentation and outline are not provided by this extension.

### Visual Studio Code

The extension in `editors/vscode` registers Fib and starts the native server.
Install `fib-lsp`, run `npm install` in the extension directory, then launch the
extension through VS Code's extension development workflow or package it using
`npx @vscode/vsce package`. Set `fib.lsp.path` to an absolute executable path
when `fib-lsp` is not on PATH. The extension does not bundle a server binary.

## Development and validation

From the repository root:

```sh
cargo fmt --check
cargo check --no-default-features --all-targets
cargo test --no-default-features
cargo clippy --no-default-features --all-targets -- -D warnings
```

`tests/lsp.rs` exercises the actual framed stdio server; frontend and analyzer
unit tests cover diagnostics, recovery and semantic tokens. The independent
Zed extension can be checked with:

```sh
cargo fmt --manifest-path lsp/editors/zed/Cargo.toml -- --check
cargo test --manifest-path lsp/editors/zed/Cargo.toml
```

The VS Code launcher tests run without an editor host:

```sh
node --test lsp/editors/vscode/test/extension.test.js
```

Automated protocol/launcher tests do not replace an interactive editor smoke test.

## Standard-library sources

Install binaries and std together with
`cargo run --no-default-features --bin fib-install -- --prefix PREFIX`.
For a source checkout, set `FIB_SYSROOT` to the absolute `library` directory or
launch `fib-lsp --sysroot /path/to/fib/library`. `fib-lsp --no-std` omits std while
retaining primitive types. Missing/mismatched sysroots produce diagnostics.
Both project planning and unsaved standalone documents use the same source and
module preparation as the CLI. See [the std guide](../docs/standard-library.md).

When editing a std source directly, the server uses the `sysroot.toml` owning that
source tree, preserving std's `root` namespace and private intrinsics. This also
works for checkout sources when the server itself is installed elsewhere. Open
sibling std files contribute unsaved overlays; enclosing application build files
do not override the std manifest.
