# Enums and Tagged Unions

Fib enums are discriminated unions: each variant is named, and a variant may optionally carry a payload (a struct of fields).

## Plain enum

```fib
type Color enum {
    Red,
    Green,
    Blue,
}
```

Construct a variant with `Type.Variant`:

```fib
c: Color = Color.Green
```

The discriminant can be read by casting:

```fib
libc::printf("color = %d\n", c as uint8)
```

## Tagged union (enum with payload)

A variant may declare fields in braces, turning it into a tagged union:

```fib
type Token enum {
    Integer { value: uint4 },
    Boolean { flag: bool },
    EOF,
}
```

Variants without fields (like `EOF` above) are constructed bare:

```fib
Token.EOF
```

Variants with fields are constructed using struct-literal syntax:

```fib
Token.Integer { value: 7 }
Token.Boolean { flag: true }
```

## Using a tagged union

Pattern-match on the variant with `switch` and `when`. See [Switch](switch.md).

```fib
fn describe(Token t) void {
    switch (t) {
        when .Integer(i) { libc::printf("int=%d\n", i.value) }
        when .Boolean(b) { libc::printf("bool=%d\n", b.flag as uint4) }
        when .EOF { libc::printf("eof\n") }
    }
}
```
