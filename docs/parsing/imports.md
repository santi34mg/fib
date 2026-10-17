# Files, qualified paths, and planned modules

> **Verified parser behavior:** Qualified names parse; source-level import declarations do not.

## Qualified syntax that parses

```fib
value := other::counter;
result := other::helpers::calculate(42);
item: other::Thing;
constructed := other::Thing { value: 1 };
```

A qualified name consists of identifier segments separated by `::`. Each segment
must be an ordinary identifier; a path cannot end in `::`. The parser stores
segments without deciding whether they denote files, declarations, or aliases.
`::` is distinct from runtime member access (`obj.field`).

A qualified struct construction records a path and field expressions.

## Old import syntax is not current grammar

The former forms below are **not supported**:

```text
import std::libc
import std::core::error::{Error}
import std::libc as c
```

The tokenizer currently treats `import` as an ordinary identifier; the statement
parser has no import production, and the statement AST has no import node.
The leftover `Keyword::Import` enum variant is not evidence of grammar support.
Selective import lists and import aliases are not parsed. `as` is currently a
cast operator, not an import alias declaration.

## Qualified names versus module declarations

`const File := other::file;` parses as an ordinary const variable declaration,
not a resolved module-alias declaration. `comp` and `pub` are parsed as unified
binding modifiers (`is_comp` and `is_pub`); namespace/visibility semantics remain
unfinished.
The proposed namespace model is detailed in [Name resolution](../name-resolution.md)
and the [roadmap](../roadmap/).

Related follow-up is collected in this [GitHub issue #9](https://github.com/santi34mg/fib/issues/9).

Parser evidence: `identifier_expressions_share_qualified_shape_and_preserve_spans`,
`qualified_callee_and_field_access_preserve_identifiers`, and
`qualified_name_missing_segment_is_an_error`.
