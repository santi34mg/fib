import assert from "node:assert/strict";
import test from "node:test";
import { highlight, semanticTokens, TOKEN_TYPES } from "../src/highlight.js";

test("highlights Fib syntax and contextual identifiers", () => {
  const source = `// greeting
import std::libc
type Message struct { value: @string }
fn greet(name: Message) @void {
  libc::printf(@concat("hi ", name.value));
  return null;
}`;
  const tokens = highlight(source);
  const pairs = tokens.map(({ text, type }) => [text, type]);

  assert.deepEqual(pairs.slice(0, 8), [
    ["// greeting", "comment"],
    ["import", "keyword"],
    ["std", "namespace"],
    ["libc", "variable"],
    ["type", "keyword"],
    ["Message", "type"],
    ["struct", "keyword"],
    ["value", "parameter"],
  ]);
  assert.ok(pairs.some(([text, type]) => text === "greet" && type === "function"));
  assert.ok(pairs.some(([text, type]) => text === "@string" && type === "type"));
  assert.ok(pairs.some(([text, type]) => text === "@concat" && type === "function"));
  assert.ok(pairs.some(([text, type]) => text === "value" && type === "property"));
  assert.ok(pairs.some(([text, type]) => text === '"hi "' && type === "string"));
});

test("uses UTF-16 columns and lengths", () => {
  const tokens = highlight("😀 fn café() @int4 {}\n");
  const fn = tokens.find((token) => token.text === "fn");
  const name = tokens.find((token) => token.text === "café");

  assert.equal(fn.start, 3);
  assert.equal(name.start, 6);
  assert.equal(name.length, 4);
});

test("splits multiline strings into valid single-line semantic tokens", () => {
  const tokens = highlight('value := "first\nsecond";');
  const strings = tokens.filter((token) => token.type === "string");

  assert.deepEqual(strings.map(({ line, start, length }) => [line, start, length]), [
    [0, 9, 6],
    [1, 0, 7],
  ]);
});

test("encodes semantic token deltas", () => {
  const data = semanticTokens("fn main() @int4 {\n  return 0;\n}");
  const keyword = TOKEN_TYPES.indexOf("keyword");
  const fn = TOKEN_TYPES.indexOf("function");

  assert.deepEqual(data.slice(0, 10), [0, 0, 2, keyword, 0, 0, 3, 4, fn, 0]);
  assert.equal(data.length % 5, 0);
});

test("does not include CRLF line endings in token ranges", () => {
  const tokens = highlight("// comment\r\nreturn 0;\r\n");

  assert.equal(tokens[0].length, "// comment".length);
  assert.equal(tokens[1].line, 1);
  assert.equal(tokens[1].start, 0);
});
