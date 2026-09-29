const fs = require("node:fs");
const path = require("node:path");
const { workspace } = require("vscode");
const { LanguageClient, TransportKind } = require("vscode-languageclient/node");

let client;

function resolveServerPath(config) {
  const configured = config.get("lsp.path");
  if (configured) return configured;

  // Packaged extensions ship a bundled copy (see scripts/copy-server.js,
  // run via the vscode:prepublish hook). During development from a checkout
  // (e.g. via F5) that copy doesn't exist yet, so fall back to the sibling
  // fib-lsp source tree instead.
  const bundled = path.join(__dirname, "..", "server", "server.js");
  if (fs.existsSync(bundled)) return bundled;
  return path.join(__dirname, "..", "..", "..", "src", "server.js");
}

function activate(context) {
  const config = workspace.getConfiguration("fib");
  const serverPath = resolveServerPath(config);

  const serverOptions = {
    run: { module: serverPath, transport: TransportKind.stdio },
    debug: { module: serverPath, transport: TransportKind.stdio },
  };

  const clientOptions = {
    documentSelector: [{ scheme: "file", language: "fib" }],
  };

  client = new LanguageClient("fibLsp", "Fib Language Server", serverOptions, clientOptions);
  context.subscriptions.push(client);
  client.start();
}

function deactivate() {
  return client ? client.stop() : undefined;
}

module.exports = { activate, deactivate };
