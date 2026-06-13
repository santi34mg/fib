# Casting

The `as` operator performs explicit conversions between types.

## Syntax

```fib
expr as Type
```

## Numeric conversions

```fib
ip as @uint8
i as @int4
ch as @char
val as @uint
```

## Pointer casts

Convert between pointer types, or between a pointer and an integer (useful for null checks and address arithmetic):

```fib
buf as *Node
block as *@int4
addr as *@void
cur as @uint8                 // pointer-to-integer
```

For a null pointer, prefer the `null` literal over integer casts — see [Pointers and Memory](pointers-memory.md).

## Enum discriminant

Reading the underlying tag of an enum value:

```fib
c: Color = Color.Green
libc::printf("%d\n", c as @uint8)
```

## Notes

`as` is an explicit conversion only — there is no implicit numeric promotion. Mixed-type arithmetic typically requires casts:

```fib
if cur as @uint == 0 as @uint { break }
```
