# Control flow

The confirmed [Boolean and switch rules](../language-specification.md) require bool
conditions and exhaustive enum switches; `&&`/`||` short-circuit left-to-right.
Explicit numeric/bool casts are implemented. Defer
syntax below is an implementation snapshot, not a newly confirmed design choice.

> **Verified parser behavior:** The constructs below parse into statement nodes.

## `if` / `else`

Conditions may be parenthesized; bodies must be brace-delimited. The entire
statement, including an `else if` chain, ends in one `;`.

```fib
if n <= 1 {
    return n;
};

if x > 0 {
    sign = 1;
} else if x < 0 {
    sign = -1;
} else {
    sign = 0;
};
```

There is no semicolon between a then-block and its `else`. Statements within
each block have their own required terminators. A bare name condition such as
`if ready { ... };` is distinguished from a struct construction; parenthesize
composite expressions when grouping needs to be explicit.

## `while`

The current loop syntax is `while (condition) { statements };`. Parentheses are
required. An empty condition is represented by `None` in the AST:

```fib
i := 0;
while (i < 10) {
    i += 1;
};

while () {
    if done { break; };
    continue;
};
```

The former C-style `for (init; cond; post)` syntax is not parsed; `for` is
currently an ordinary identifier. There is no parsed iterator/range loop either.

## `break` and `continue`

`break;` and `continue;` produce bare control-flow statement nodes. The parser
does not check whether they occur inside a loop. Named targets such as
`break ::outer;` are not parsed. See [Name resolution](../name-resolution.md) for
the separate, planned label namespace.

## `return`

```fib
fn divmod(a: int32, b: int32) (int32, int32) {
    return a / b, a % b;
}
fn finish() void {
    return;
}
```

A return records no value or a comma-separated expression list.

## `defer`

```fib
defer cleanup();
defer {
    cleanup();
    release();
};
defer if ready { cleanup(); };
```

`defer` wraps a statement or a brace-delimited block. One final `;` terminates
the outer defer. Each statement inside a deferred block needs its own terminator.
Deferred blocks have a lexical scope for their local declarations.

## `switch`

`switch (subject) { when ... { ... } };` is statement syntax, not an expression.
See [Switch](switch.md) for patterns and payload bindings.

Related follow-up is collected in this [GitHub issue #14](https://github.com/santi34mg/fib/issues/14).

Parser coverage: `test_if_statement`, `test_if_else_statement`,
`test_if_name_condition_is_not_struct_construct`, `test_while_loop`,
`test_while_without_condition`, `test_break_continue`, `test_return_void`, and
`test_defer_statement`, `defer_blocks_preserve_statements_and_spans`, and
`malformed_defer_blocks_are_rejected`.
