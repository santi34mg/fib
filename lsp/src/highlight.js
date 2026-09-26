export const TOKEN_TYPES = [
  "namespace",
  "type",
  "parameter",
  "variable",
  "property",
  "enumMember",
  "function",
  "keyword",
  "comment",
  "string",
  "number",
  "operator",
  "macro",
];

const tokenTypeIndex = new Map(TOKEN_TYPES.map((type, index) => [type, index]));

const keywords = new Set([
  "var",
  "fn",
  "switch",
  "when",
  "type",
  "struct",
  "enum",
  "union",
  "if",
  "else",
  "while",
  "break",
  "continue",
  "return",
  "as",
  "extern",
  "defer",
  "import",
  "true",
  "false",
  "null",
]);

const builtinTypes = new Set([
  "void",
  "uint",
  "uint2",
  "uint4",
  "uint8",
  "uint16",
  "int",
  "int2",
  "int4",
  "int8",
  "int16",
  "float2",
  "float4",
  "float8",
  "float16",
  "char",
  "bool",
  "string",
  "never",
  "usize",
]);

const builtinFunctions = new Set(["concat", "str_len", "str_eq"]);
const builtinProperties = new Set(["len"]);
const twoCharacterOperators = new Set([
  "==",
  "!=",
  ">=",
  "<=",
  ">>",
  "<<",
  "+=",
  "-=",
  "*=",
  "/=",
  "%=",
  "&&",
  "||",
  "->",
  "..",
]);
const operatorCharacters = new Set("=!><+-*/%&|^~");
const punctuationCharacters = new Set("(){}[],;:.");
const identifierStart = /[\p{L}_]/u;
const identifierContinue = /[\p{L}\p{N}_]/u;

function codePointAt(text, offset) {
  const value = String.fromCodePoint(text.codePointAt(offset));
  return [value, value.length];
}

function scanIdentifier(line, start) {
  let offset = start;
  while (offset < line.length) {
    const [character, width] = codePointAt(line, offset);
    if (!identifierContinue.test(character)) break;
    offset += width;
  }
  return offset;
}

function scanQuoted(line, start, quote) {
  let offset = start + 1;
  let escaped = false;
  while (offset < line.length) {
    const [character, width] = codePointAt(line, offset);
    offset += width;
    if (escaped) {
      escaped = false;
    } else if (character === "\\") {
      escaped = true;
    } else if (character === quote) {
      return [offset, true];
    }
  }
  return [offset, false];
}

function scanNumber(line, start) {
  let offset = start;
  if (line[offset] === "0" && ["x", "d", "o", "b"].includes(line[offset + 1])) {
    offset += 2;
    const base = line[start + 1];
    const digit =
      base === "x"
        ? /[0-9a-f]/i
        : base === "o"
          ? /[0-7]/
          : base === "b"
            ? /[01]/
            : /[0-9]/;
    while (offset < line.length && digit.test(line[offset])) offset += 1;
    return offset;
  }

  while (offset < line.length && /[0-9]/.test(line[offset])) offset += 1;
  if (line[offset] === "." && /[0-9]/.test(line[offset + 1] ?? "")) {
    offset += 1;
    while (offset < line.length && /[0-9]/.test(line[offset])) offset += 1;
  }
  return offset;
}

function rawTokens(text) {
  const tokens = [];
  const lines = text.split("\n").map((line) => (line.endsWith("\r") ? line.slice(0, -1) : line));
  let continuedString = false;

  for (let lineNumber = 0; lineNumber < lines.length; lineNumber += 1) {
    const line = lines[lineNumber];
    let offset = 0;

    if (continuedString) {
      const [end, closed] = scanQuoted(line, -1, '"');
      if (end > 0) tokens.push({ line: lineNumber, start: 0, length: end, type: "string", text: line.slice(0, end) });
      offset = end;
      continuedString = !closed;
      if (continuedString) continue;
    }

    while (offset < line.length) {
      const [character, width] = codePointAt(line, offset);
      if (/\s/u.test(character)) {
        offset += width;
        continue;
      }

      const start = offset;
      if (line.startsWith("//", offset)) {
        tokens.push({ line: lineNumber, start, length: line.length - start, type: "comment", text: line.slice(start) });
        break;
      }

      if (character === '"') {
        const [end, closed] = scanQuoted(line, start, character);
        tokens.push({ line: lineNumber, start, length: end - start, type: "string", text: line.slice(start, end) });
        offset = end;
        continuedString = !closed;
        if (continuedString) break;
        continue;
      }

      if (character === "'") {
        const [end] = scanQuoted(line, start, character);
        tokens.push({ line: lineNumber, start, length: end - start, type: "string", text: line.slice(start, end) });
        offset = end;
        continue;
      }

      if (/[0-9]/.test(character)) {
        const end = scanNumber(line, start);
        tokens.push({ line: lineNumber, start, length: end - start, type: "number", text: line.slice(start, end) });
        offset = end;
        continue;
      }

      if (character === "@") {
        offset += width;
        const end = offset < line.length && identifierStart.test(codePointAt(line, offset)[0]) ? scanIdentifier(line, offset) : offset;
        const name = line.slice(offset, end);
        const type = builtinTypes.has(name)
          ? "type"
          : builtinFunctions.has(name)
            ? "function"
            : builtinProperties.has(name)
              ? "property"
              : "macro";
        tokens.push({ line: lineNumber, start, length: end - start, type, text: line.slice(start, end) });
        offset = end;
        continue;
      }

      if (identifierStart.test(character)) {
        const end = scanIdentifier(line, start);
        const word = line.slice(start, end);
        tokens.push({ line: lineNumber, start, length: end - start, type: keywords.has(word) ? "keyword" : null, text: word });
        offset = end;
        continue;
      }

      const three = line.slice(offset, offset + 3);
      const two = line.slice(offset, offset + 2);
      if (three === "..." || twoCharacterOperators.has(two) || operatorCharacters.has(character)) {
        const value = three === "..." ? three : twoCharacterOperators.has(two) ? two : character;
        tokens.push({ line: lineNumber, start, length: value.length, type: "operator", text: value });
        offset += value.length;
        continue;
      }

      if (two === "::") {
        tokens.push({ line: lineNumber, start, length: 2, type: "punctuation", text: two });
        offset += 2;
        continue;
      }

      if (punctuationCharacters.has(character)) {
        tokens.push({ line: lineNumber, start, length: width, type: "punctuation", text: character });
      }
      offset += width;
    }
  }

  return tokens;
}

function classifyIdentifiers(tokens) {
  for (let index = 0; index < tokens.length; index += 1) {
    const token = tokens[index];
    if (token.type !== null) continue;

    const previous = tokens[index - 1];
    const next = tokens[index + 1];
    if (previous?.text === "fn") token.type = "function";
    else if (previous?.text === "type") token.type = "type";
    else if (previous?.text === ".") token.type = /^[A-Z]/.test(token.text) ? "enumMember" : "property";
    else if (next?.text === "::") token.type = "namespace";
    else if (next?.text === "(") token.type = "function";
    else if (previous?.text === "as" || /^[A-Z]/.test(token.text)) token.type = "type";
    else if (next?.text === ":" && previous?.text !== ".") token.type = "parameter";
    else token.type = "variable";
  }
}

export function highlight(text) {
  const tokens = rawTokens(text);
  classifyIdentifiers(tokens);
  return tokens.filter((token) => token.type !== "punctuation");
}

export function semanticTokens(text) {
  const data = [];
  let previousLine = 0;
  let previousStart = 0;

  for (const token of highlight(text)) {
    const deltaLine = token.line - previousLine;
    const deltaStart = deltaLine === 0 ? token.start - previousStart : token.start;
    data.push(deltaLine, deltaStart, token.length, tokenTypeIndex.get(token.type), 0);
    previousLine = token.line;
    previousStart = token.start;
  }

  return data;
}
