const fs = require("node:fs");
const path = require("node:path");

const src = path.join(__dirname, "..", "..", "..", "src");
const dest = path.join(__dirname, "..", "server");

fs.rmSync(dest, { recursive: true, force: true });
fs.cpSync(src, dest, { recursive: true });

console.log(`fib-lsp: copied ${src} -> ${dest}`);
