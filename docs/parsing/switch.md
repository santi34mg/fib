# Switch

> **Verified parser behavior:** Switch statements and variant/wildcard patterns parse.

## Statement syntax

```fib
fn inspect(color: Color) void {
    switch (color) {
        when .Red { code := 1; }
        when .Green { code := 2; }
        when .Blue { code := 3; }
    };
}
```

Parentheses around the subject are required, as are braces around the switch and
each arm body. Each arm starts with `when`. The complete switch statement ends
with `;`; individual arm blocks have no extra terminator between arms. Statements
inside each arm have their own mandatory semicolons. An empty arm or empty switch
body parses. `switch` is not an expression that returns a value.

## Payload binding

```fib
switch (token) {
    when .Integer(payload) { value := payload.value; }
    when .Boolean(payload) { flag := payload.flag; }
    when .EOF { }
};
```

A variant pattern is `.Name` with an optional single `(binding)` identifier.
It records a binding node, not a destructuring expression or a list of fields.
The parser preserves the variant name and optional binding, with semantic
annotations left pending.

## Wildcard

```fib
switch (color) {
    when .Red { code := 1; }
    when else { code := 0; }
};
```

`when else` is the parsed catch-all form; the parser does not require it to be
last or unique. Despite a stale source comment mentioning `_`, `when _` is not
implemented.

Other unsupported pattern syntax includes literal/range patterns, qualified
variant names, nested patterns, multiple bindings, guards, and OR patterns.
See [Enums](enums.md) for declaration and construction limitations.

Related follow-up is collected in this [GitHub issue #14](https://github.com/santi34mg/fib/issues/14).

Parser coverage: `test_switch_full_shape`,
`switch_payload_binding_preserves_variant_and_body`,
`malformed_composite_types_signatures_and_patterns_error`, and
`parsed_pattern_payload_and_variant_annotations_are_pending`.
