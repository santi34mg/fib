import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import test from "node:test";

function frame(message) {
  const body = JSON.stringify(message);
  return `Content-Length: ${Buffer.byteLength(body)}\r\n\r\n${body}`;
}

test("serves initialize and semantic token requests", async (context) => {
  const server = spawn(process.execPath, ["src/server.js"], { stdio: ["pipe", "pipe", "pipe"] });
  context.after(() => server.kill());
  let output = Buffer.alloc(0);
  const messages = [];

  server.stdout.on("data", (chunk) => {
    output = Buffer.concat([output, chunk]);
    while (true) {
      const end = output.indexOf("\r\n\r\n");
      if (end === -1) return;
      const length = Number(/Content-Length:\s*(\d+)/i.exec(output.subarray(0, end).toString())[1]);
      if (output.length < end + 4 + length) return;
      messages.push(JSON.parse(output.subarray(end + 4, end + 4 + length).toString()));
      output = output.subarray(end + 4 + length);
    }
  });

  const waitForMessages = (count) =>
    new Promise((resolve, reject) => {
      const timeout = setTimeout(() => reject(new Error(`timed out waiting for ${count} responses`)), 2000);
      const poll = setInterval(() => {
        if (messages.length >= count) {
          clearTimeout(timeout);
          clearInterval(poll);
          resolve();
        }
      }, 5);
    });

  server.stdin.write(frame({ jsonrpc: "2.0", id: 1, method: "initialize", params: {} }));
  await waitForMessages(1);
  assert.equal(messages[0].result.serverInfo.name, "fib-lsp");
  assert.equal(messages[0].result.capabilities.semanticTokensProvider.full, true);

  const uri = "file:///tmp/example.fib";
  server.stdin.write(frame({
    jsonrpc: "2.0",
    method: "textDocument/didOpen",
    params: { textDocument: { uri, languageId: "fib", version: 1, text: "fn main() @int4 {}" } },
  }));
  server.stdin.write(frame({
    jsonrpc: "2.0",
    id: 2,
    method: "textDocument/semanticTokens/full",
    params: { textDocument: { uri } },
  }));
  await waitForMessages(2);
  assert.ok(messages[1].result.data.length > 0);

  server.stdin.write(frame({ jsonrpc: "2.0", id: 3, method: "shutdown", params: null }));
  await waitForMessages(3);
  server.stdin.write(frame({ jsonrpc: "2.0", method: "exit", params: null }));
  await new Promise((resolve) => server.once("exit", resolve));
  assert.equal(server.exitCode, 0);
});
