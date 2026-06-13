# Operators

## Arithmetic

`+`, `-`, `*`, `/`, `%`

## Comparison

`==`, `!=`, `<`, `<=`, `>`, `>=`

## Logical

`&&` (and), `||` (or), `!` (not)

## Bitwise

`&` (and), `|` (or), `^` (xor), `~` (not), `<<` (left shift), `>>` (right shift)

```fib
return (a << 24) | (b << 16) | (c << 8) | d
perms = perms & ~PERM_WRITE
```

## Assignment

- `=` — plain assignment
- `+=`, `-=`, `*=`, `/=`, `%=` — compound assignment

```fib
count += 1
i -= 1
```

## Postfix / pointer operators

These trail the expression they apply to:

- `.&` — address-of (produces a pointer to the operand)
- `.*` — dereference (loads the pointee)
- `.[i]` — index into a pointer/array
- `.field` — field access on a struct (or via a pointer)

```fib
pool.&                  // pointer to pool
cur.*                   // value pointed to by cur
arr.[i]                 // element i
o.inner.value           // chained field access
```

## Cast

`expr as Type` — explicit type conversion. See [Casting](casting.md).

```fib
ch as @char
buf as *Node
i as @uint8
```

## Other tokens

- `->` appears only in function type expressions: `fn(@int4) -> @int4`. See [Functions](functions.md).
- `@` prefixes builtins: `@int`, `@string`, `@concat`, ... See [Types](types.md) and [Builtins](builtins.md).

## Reserved / not-yet-consumed

- `..` (range syntax) is reserved; the parser does not consume it yet.
