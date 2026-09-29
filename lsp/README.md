# fib-lsp

A small, dependency-free language server for the [Fib language](../fib). It
provides semantic syntax highlighting for:

- keywords, comments, strings, numbers, and operators
- builtin types, functions, and properties
- functions, types, namespaces, variables, fields, and enum members

## Requirements

Node.js 20 or newer.

## Run

Run the server over standard input/output:

```sh
node /path/to/fib-lsp/src/server.js
```

To install the `fib-lsp` command globally from a checkout:

```sh
npm install --global /path/to/fib-lsp
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

### Visual Studio Code

Unlike Neovim and Helix, VS Code has no built-in way to attach an LSP server
by configuration alone, so a minimal extension lives at
[`editors/vscode`](editors/vscode). It registers the `fib` language for
`.fib` files and spawns `../../src/server.js` over stdio via
`vscode-languageclient`.

To use it from a checkout:

```sh
cd /path/to/fib-lsp/editors/vscode
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
