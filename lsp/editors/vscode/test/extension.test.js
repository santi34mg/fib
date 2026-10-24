const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const test = require("node:test");

for (const configured of ["", "/checkout with spaces/fib/target/debug/fib-lsp"]) {
  test(`launches native executable ${configured || "from PATH"} and watches sources`, async () => {
    const watcher = { dispose() {} };
    let options;
    let started = false;
    let stopped = false;
    const context = { subscriptions: [] };
    const module = { exports: {} };
    vm.runInNewContext(fs.readFileSync(path.join(__dirname, "../src/extension.js"), "utf8"), {
      module,
      require(name) {
        if (name === "vscode") return { workspace: {
          getConfiguration: () => ({ get: () => configured }),
          createFileSystemWatcher(pattern) { assert.equal(pattern, "**/*.fib"); return watcher; },
        } };
        assert.equal(name, "vscode-languageclient/node");
        return { LanguageClient: class {
          constructor(id, title, server, client) { options = { server, client }; }
          start() { started = true; }
          stop() { stopped = true; return Promise.resolve(); }
        } };
      },
    });
    module.exports.activate(context);
    assert.equal(options.server.command, configured || "fib-lsp");
    assert.equal(options.server.args.length, 0);
    assert.equal(options.server.module, undefined);
    assert.equal(options.client.synchronize.fileEvents, watcher);
    assert.equal(started, true);
    assert.equal(context.subscriptions.length, 2);
    await module.exports.deactivate();
    assert.equal(stopped, true);
  });
}
