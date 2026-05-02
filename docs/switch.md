# Switch

`switch` matches a value against a set of patterns. Each arm is introduced with `when`.

## Matching enum variants

Variants are matched with the dotted pattern `.Variant`:

```fib
fn describe(Color c) void {
    switch (c) {
        when .Red   { libc::printf("red\n") }
        when .Green { libc::printf("green\n") }
        when .Blue  { libc::printf("blue\n") }
    }
}
```

## Binding a payload

For a variant carrying a payload, bind it with `(name)`. The bound name behaves like a struct of the variant's fields:

```fib
fn describe(Token t) void {
    switch (t) {
        when .Integer(i) { libc::printf("int=%d\n", i.value) }
        when .Boolean(b) { libc::printf("bool=%d\n", b.flag as uint4) }
        when .EOF        { libc::printf("eof\n") }
    }
}
```

## Wildcard

`_` matches anything (catch-all arm). Use it as the final arm to handle remaining variants.

## Arm bodies

Each arm's body is a brace-delimited block — any sequence of statements is allowed inside.
