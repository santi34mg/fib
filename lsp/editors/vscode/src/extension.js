const { workspace } = require("vscode");
const { LanguageClient } = require("vscode-languageclient/node");

let client;

function activate(context) {
  const config = workspace.getConfiguration("fib");
  const command = config.get("lsp.path") || "fib-lsp";
  const watcher = workspace.createFileSystemWatcher("**/*.fib");
  context.subscriptions.push(watcher);
  client = new LanguageClient("fibLsp", "Fib Language Server", {
    command,
    args: [],
  }, {
    documentSelector: [{ scheme: "file", language: "fib" }],
    synchronize: { fileEvents: watcher },
  });
  context.subscriptions.push(client);
  client.start();
}

function deactivate() {
  return client ? client.stop() : undefined;
}

module.exports = { activate, deactivate };
