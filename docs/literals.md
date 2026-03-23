# Literals

## Integer Literals

Integer literals are written in decimal by default. Underscores may be used to separate digits visually.

```
42
100_000
```

Other bases are specified with a prefix:

| Prefix | Base        | Example    |
|--------|-------------|------------|
| `0x`   | Hexadecimal | `0xFF`     |
| `0o`   | Octal       | `0o755`    |
| `0b`   | Binary      | `0b1010`   |

## Float Literals

Float literals contain a decimal point:

```
3.14
0.5
1.0e10
```

## Boolean Literals

```
true
false
```

## Character Literals

A character literal is a single character enclosed in single quotes:

```
'a'
'\n'
```

### Escape Sequences

| Sequence    | Character              |
|-------------|------------------------|
| `\n`        | Newline                |
| `\r`        | Carriage return        |
| `\t`        | Tab                    |
| `\\`        | Backslash              |
| `\'`        | Single quote           |
| `\"`        | Double quote           |
| `\0`        | Null                   |
| `\xNN`      | Byte value (hex)       |
| `\u{NNNN}`  | Unicode code point     |

## String Literals

A string literal is a sequence of characters enclosed in double quotes. String literals support the same escape sequences as character literals.

```
"hello, world"
"line one\nline two"
"unicode: \u{1F600}"
```

## Null

The `null` literal represents a null pointer value:

```
null
```
