import assert from "node:assert/strict";
import test from "node:test";
import { highlight, semanticTokens, TOKEN_TYPES } from "../src/highlight.js";

test("highlights Fib syntax and contextual identifiers", () => {
  const source = `// greeting
type Message struct { value: string }
fn greet(name: Message) void {
  libc::printf(@concat("hi ", name.value));
  return null;
}`;
  const tokens = highlight(source);
  const pairs = tokens.map(({ text, type }) => [text, type]);

  assert.deepEqual(pairs.slice(0, 5), [
    ["// greeting", "comment"],
    ["type", "type"],
    ["Message", "type"],
    ["struct", "keyword"],
    ["value", "property"],
  ]);
  assert.ok(pairs.some(([text, type]) => text === "greet" && type === "function"));
  assert.ok(pairs.some(([text, type]) => text === "string" && type === "type"));
  assert.ok(pairs.some(([text, type]) => text === "@concat" && type === "function"));
  assert.ok(pairs.some(([text, type]) => text === "value" && type === "property"));
  assert.ok(pairs.some(([text, type]) => text === '"hi "' && type === "string"));
});

test("uses UTF-16 columns and lengths", () => {
  const tokens = highlight("😀 fn café() int32 {}\n");
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
  const data = semanticTokens("fn main() int32 {\n  return 0;\n}");
  const keyword = TOKEN_TYPES.indexOf("keyword");
  const fn = TOKEN_TYPES.indexOf("function");

  assert.deepEqual(data.slice(0, 10), [0, 0, 2, keyword, 0, 0, 3, 4, fn, 0]);
  assert.equal(data.length % 5, 0);
});

test("matches current keywords, builtin types and properties", () => {
  const pairs = highlight("const count: uint64 = 0d12.5; var import int4 float8 @int32 @len value.len").map(({ text, type }) => [text, type]);
  for (const pair of [["const", "keyword"], ["count", "variable"], ["uint64", "type"], ["0d12.5", "number"], ["var", "variable"], ["import", "variable"], ["int4", "variable"], ["float8", "variable"], ["@int32", "macro"], ["@len", "macro"], ["len", "property"]]) {
    assert.ok(pairs.some(([text, type]) => text === pair[0] && type === pair[1]), JSON.stringify(pair));
  }
});

test("distinguishes declaration contexts without capitalization guesses", () => {
  const tokens = highlight(`type // declaration comment
lower struct { field: int32 }
type choice enum { empty, payload { item: lower } }
fn use(arg: lower) void { const Upper := 1; local: lower; value.Field; }`);
  for (const [text, type] of [["lower", "type"], ["empty", "enumMember"], ["payload", "enumMember"], ["item", "property"], ["arg", "parameter"], ["Upper", "variable"], ["local", "variable"], ["Field", "property"]]) {
    assert.ok(tokens.some((token) => token.text === text && token.type === type), `${text}: ${type}`);
  }
});

test("does not include CRLF line endings in token ranges", () => {
  const tokens = highlight("// comment\r\nreturn 0;\r\n");

  assert.equal(tokens[0].length, "// comment".length);
  assert.equal(tokens[1].line, 1);
  assert.equal(tokens[1].start, 0);
});
