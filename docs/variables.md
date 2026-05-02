# Variables

Fib provides explicit and inferred local variable declarations.

## Explicit declaration

`name: type = expr` — declares a variable with an explicit type and initializer.

```fib
count: int4 = 0
v: uint4 = x
head: *Node
```

The initializer is optional; `head: *Node` declares an uninitialized pointer.

## Inferred declaration

`name := expr` — the variable's type is inferred from the right-hand side.

```fib
q, r := divmod(17, 5)
total: uint8 = block_size * capacity
```

The walrus form also supports multiple identifiers, used for destructuring multi-return functions.

## Assignment

After declaration, assign with `=`:

```fib
perms = perms | PERM_EXEC
o.inner.value = 42
arr.[j] = arr.[j - 1]
cur.* = next_value
```

Compound assignment is supported: `+=`, `-=`, `*=`, `/=`, `%=`. See [Operators](operators.md).

## Multiple assignment

When a function returns multiple values, you can assign to several targets at once:

```fib
q, r = divmod(17, 5)
```
