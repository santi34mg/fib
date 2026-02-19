# Character literals

A character literal represents a single Unicode code point.

**Syntax**:

```
'<unicode-code-point>'
```

Escape sequences include:

| Sequence   | Meaning                |
| ---------- | ---------------------- |
| `\n`       | Newline                |
| `\r`       | Carriage return        |
| `\t`       | Tab                    |
| `\\`       | Backslash              |
| `\'`       | Single quote           |
| `\0`       | Null character         |
| `\xNN`     | Hexadecimal byte value |
| `\u{NNNN}` | Unicode code point     |

**Examples**:

```
'a'
'\n'
'\u{03B1}'  // Greek letter alpha
```
