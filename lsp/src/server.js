#!/usr/bin/env node

import process from "node:process";
import { semanticTokens, TOKEN_TYPES } from "./highlight.js";

const documents = new Map();
let input = Buffer.alloc(0);
let shutdownRequested = false;

function send(message) {
  const body = JSON.stringify(message);
  process.stdout.write(`Content-Length: ${Buffer.byteLength(body)}\r\n\r\n${body}`);
}

function respond(id, result) {
  send({ jsonrpc: "2.0", id, result });
}

function respondError(id, code, message) {
  send({ jsonrpc: "2.0", id, error: { code, message } });
}

function handle(message) {
  const { id, method, params } = message;

  switch (method) {
    case "initialize":
      respond(id, {
        capabilities: {
          positionEncoding: "utf-16",
          textDocumentSync: 1,
          semanticTokensProvider: {
            legend: { tokenTypes: TOKEN_TYPES, tokenModifiers: [] },
            full: true,
          },
        },
        serverInfo: { name: "fib-lsp", version: "0.1.0" },
      });
      break;
    case "initialized":
      break;
    case "shutdown":
      shutdownRequested = true;
      respond(id, null);
      break;
    case "exit":
      process.exit(shutdownRequested ? 0 : 1);
      break;
    case "textDocument/didOpen":
      documents.set(params.textDocument.uri, params.textDocument.text);
      break;
    case "textDocument/didChange": {
      const change = params.contentChanges.at(-1);
      if (change && typeof change.text === "string") documents.set(params.textDocument.uri, change.text);
      break;
    }
    case "textDocument/didClose":
      documents.delete(params.textDocument.uri);
      break;
    case "textDocument/semanticTokens/full": {
      const text = documents.get(params.textDocument.uri);
      if (text === undefined) respondError(id, -32602, "document is not open");
      else respond(id, { data: semanticTokens(text) });
      break;
    }
    default:
      if (id !== undefined) respondError(id, -32601, `method not found: ${method}`);
  }
}

function parseInput() {
  while (true) {
    const headerEnd = input.indexOf("\r\n\r\n");
    if (headerEnd === -1) return;

    const header = input.subarray(0, headerEnd).toString("ascii");
    const match = /^Content-Length:\s*(\d+)$/im.exec(header);
    if (!match) {
      console.error("fib-lsp: received a message without Content-Length");
      input = input.subarray(headerEnd + 4);
      continue;
    }

    const length = Number(match[1]);
    const bodyStart = headerEnd + 4;
    if (input.length < bodyStart + length) return;

    const body = input.subarray(bodyStart, bodyStart + length).toString("utf8");
    input = input.subarray(bodyStart + length);
    try {
      handle(JSON.parse(body));
    } catch (error) {
      console.error(`fib-lsp: ${error.message}`);
    }
  }
}

process.stdin.on("data", (chunk) => {
  input = Buffer.concat([input, chunk]);
  parseInput();
});
