# fib-lsp

A small, dependency-free language server for the [Fib language](../README.md). It
provides semantic syntax highlighting for:

- keywords, comments, strings, numbers, and operators
- builtin types, functions, and properties
- functions, types, namespaces, variables, fields, and enum members

Highlighting follows the current tokenizer/parser syntax: bare builtin types
(`int32`, `uint64`, `string`), `const` and `name := value` bindings, and `.len`.
Builtin calls retain the `@` prefix (`@concat`, `@str_len`, `@str_eq`).
Legacy `var`/`import` keywords and `@int4`-style types are not supported.
Identifier classification is syntax-based, not compiler-backed name resolution;
the server does not provide parsing or type-checking diagnostics.

## Requirements

Node.js 20 or newer.

## Run

Run the server over standard input/output:

```sh
node /path/to/fib/lsp/src/server.js
```

To install the `fib-lsp` command globally from a checkout:

```sh
npm install --global /path/to/fib/lsp
```

## Editor setup

### Neovim 0.11+

Add this to your configuration, replacing the command path if the package is
not installed globally:

```lua
vim.filetype.add({ extension = { fib = "fib" } })

vim.lsp.config("fib_lsp", {
  cmd = { "fib-lsp" },
  filetypes = { "fib" },
  root_markers = { ".git" },
})
vim.lsp.enable("fib_lsp")
```

### Helix

Add this to `~/.config/helix/languages.toml`:

```toml
[language-server.fib-lsp]
command = "fib-lsp"

[[language]]
name = "fib"
scope = "source.fib"
file-types = ["fib"]
roots = [".git"]
language-servers = ["fib-lsp"]
```

An editor must have a grammar or basic language entry for `.fib` files before
it can attach any language server. Semantic-token-capable clients then use the
server's highlighting without a TextMate or tree-sitter grammar.

### Zed

A local Rust/Wasm extension lives at [`editors/zed`](editors/zed). It registers
`Fib` for `.fib` files and launches the existing JS server over stdio. It does
not bundle or download a server, Node runtime, or Tree-sitter grammar.

1. Install a current Zed release and Node.js 20+.
2. Make the server available with `npm install --global /path/to/fib/lsp`, with
   both `fib-lsp` and `node` on Zed's worktree PATH. Alternatively, skip the
   global installation and use the binary override below.
3. Install Rust through [rustup](https://rustup.rs/), then prepare the extension
   toolchain:

   ```sh
   rustup toolchain install stable
   rustup target add --toolchain stable wasm32-wasip2
   ```

   This extension is an independent Cargo package and does not build Fib or
   LLVM. Its local `rust-toolchain.toml` selects stable Rust with
   `wasm32-wasip2`, rather than inheriting the compiler repository's pinned
   toolchain. The Wasm target must be installed for the same toolchain used to
   build the extension. Fully quit and reopen Zed after installing rustup or
   changing PATH so it inherits the updated environment.
4. Run `zed: install dev extension` from Zed's command palette (or use
   **Extensions → Install Dev Extension**) and select
   `/path/to/fib/lsp/editors/zed`, **not** the repository root or `lsp/`.
   The initial build needs access to crates.io for Rust dependencies.
5. Enable LSP highlighting in Zed's settings:

   ```json
   {
     "languages": {
       "Fib": {
         "semantic_tokens": "full"
       }
     }
   }
   ```

To run directly from the checkout without a global npm installation, merge this
into the same settings file, replacing both paths with absolute paths on the
machine running the language server:

```json
{
  "lsp": {
    "fib-lsp": {
      "binary": {
        "path": "/absolute/path/to/node",
        "arguments": ["/absolute/path/to/fib/lsp/src/server.js"]
      }
    }
  }
}
```

`binary.path` is the executable, not a JS file or a shell command string;
`binary.arguments` is passed verbatim as separate arguments (paths with spaces
need no shell quoting). You can also point it at an installed `fib-lsp`
executable without arguments. Optional `binary.env` entries override the
worktree shell environment. Without a path override, the extension looks up
`fib-lsp` using Zed's worktree PATH and preserves that environment so the
`#!/usr/bin/env node` launcher can find Node. It deliberately does not infer the
checkout location from the extension's Wasm working directory.

Open or reopen a `.fib` file and confirm the language picker says **Fib**.
If your settings restrict `language_servers`, include `"fib-lsp"` under
`languages.Fib.language_servers`. The server should appear in Zed's language
server status; try `fn main() int32 { return 1; }` to check highlighting, then
edit the file to check updates. After changing the launch settings, run
`editor: restart language server`. Use `zed: open log` for build/startup errors;
check Node's version and the absolute binary paths if startup fails.

#### Troubleshooting Zed installation

**System Rust already installed (for example, Void Linux's `/usr/bin/cargo`).**
Rustup can coexist with the system installation; uninstalling distro packages is
not required. If the official installer refuses because Rust is already
installed, download and run it with the path check explicitly skipped:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o /tmp/rustup-init.sh
RUSTUP_INIT_SKIP_PATH_CHECK=yes sh /tmp/rustup-init.sh
```

After completing the installer, load its environment and check command lookup:

```sh
. "$HOME/.cargo/env"
command -v cargo
command -v rustup
```

Cargo should resolve to `$HOME/.cargo/bin/cargo`, not `/usr/bin/cargo`. If it
still resolves to the system executable, put rustup first in PATH:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
```

For zsh, run `rehash` and add that export at the end of `~/.zshrc`, after other
PATH changes. Other shells should use their corresponding startup file. Fully
quit Zed and launch it from the corrected terminal with `zed /path/to/fib`.

**`Failed to install dev extension: compiling Rust extension`.** This is a
summary, not the underlying error. Run `zed: open log` and inspect the following
`Caused by` message. Common causes:

- **Target not installed / rustup unavailable:** ensure Zed can find rustup and
  run the stable toolchain/target installation commands above.
- **`can't find crate for core`:** the selected toolchain lacks the Wasm target.
  Do not remove the extension's local `rust-toolchain.toml`: without it, the
  parent repository's compiler pin can select a different toolchain from stable.
- **Dependency downloads fail:** the initial build needs network access to
  crates.io, even though the JS language server itself has no npm dependencies.

To isolate compilation from Zed, run from the repository root:

```sh
cargo +stable build --manifest-path lsp/editors/zed/Cargo.toml --target wasm32-wasip2
```

Once this succeeds, retry `zed: install dev extension` with `lsp/editors/zed`.

**`org.freedesktop.portal.Desktop was not provided by any .service files`.**
In the observed session, this warning came from Zed trying to prevent idle sleep,
not from extension compilation. It does not block Fib highlighting. To resolve
it, install `xdg-desktop-portal` and the appropriate backend for your desktop
(e.g. GTK, KDE, or wlroots), ensure the desktop session has a working session
D-Bus, then log out and back in. Manually launched window-manager sessions may
need `dbus-run-session`; follow your desktop/distribution's setup instructions.

#### Support and validation

This extension intentionally omits `grammar`. Although the
[language-extension docs](https://zed.dev/docs/extensions/languages) call it
required, Zed source reviewed at revision
[`322cd509`](https://github.com/zed-industries/zed/tree/322cd509dcccaee1f6bc44725d270598ae8171ad)
accepts it as optional in
[`LanguageConfig`](https://github.com/zed-industries/zed/blob/322cd509dcccaee1f6bc44725d270598ae8171ad/crates/language_core/src/language_config.rs)
and explicitly loads languages without a grammar in
[`LanguageRegistry`](https://github.com/zed-industries/zed/blob/322cd509dcccaee1f6bc44725d270598ae8171ad/crates/language/src/language_registry.rs).
The [extension host](https://github.com/zed-industries/zed/blob/322cd509dcccaee1f6bc44725d270598ae8171ad/crates/extension_host/src/extension_host.rs)
also passes the optional grammar through when registering extension languages.
Highlighting therefore requires `semantic_tokens: "full"` (off by default in
Zed); without the server there is no syntax coloring. There are no Tree-sitter
queries, syntax-aware indentation, outline, or structural navigation. The LSP
still only provides syntax-based semantic tokens, not diagnostics, completion,
or compiler-backed analysis. The Wasm build succeeded with stable Rust, and a
user confirmed successful local installation and highlighting on Void Linux on
2026-10-17 after the toolchain fix. Older Zed versions may not support these
settings; no minimum Zed release has been established.
See also [Zed's dev-extension instructions](https://zed.dev/docs/extensions/developing-extensions).

### Visual Studio Code

Unlike Neovim and Helix, VS Code has no built-in way to attach an LSP server
by configuration alone, so a minimal extension lives at
[`editors/vscode`](editors/vscode). It registers the `fib` language for
`.fib` files and spawns `../../src/server.js` over stdio via
`vscode-languageclient`.

To use it from a checkout:

```sh
cd /path/to/fib/lsp/editors/vscode
npm install
```

Then either:

- Open `editors/vscode` in VS Code and press `F5` to launch an Extension
  Development Host with it loaded, or
- Package and install it permanently:

  ```sh
  npx @vscode/vsce package
  code --install-extension fib-lsp-vscode-0.1.0.vsix
  ```

If `fib-lsp` isn't at `../../src/server.js` relative to the extension (e.g.
you installed it globally), set `fib.lsp.path` in your VS Code settings to
the absolute path of `server.js` or the `fib-lsp` binary.

## Development

```sh
npm test
```

Check the independent Zed extension (no LLVM required):

```sh
cargo fmt --manifest-path editors/zed/Cargo.toml -- --check
cargo test --manifest-path editors/zed/Cargo.toml --no-default-features
cargo build --manifest-path editors/zed/Cargo.toml --target wasm32-wasip2 --release
```

The Rust tests cover command/PATH selection, argument forwarding, environment
merging, and the missing-server error. They do not replace the interactive Zed
smoke check above; native Cargo tests do not exercise Zed's host API.
