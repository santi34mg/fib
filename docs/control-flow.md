# Control Flow

## `if` / `else`

Parentheses around the condition are not required. The body is a brace-delimited block.

```fib
if n <= 1 {
    ret n
}

if (perms & PERM_READ) != 0 {
    libc::printf("  [x] READ\n")
} else {
    libc::printf("  [ ] READ\n")
}
```

## `for`

C-style for loop with three semicolon-separated clauses: initializer, condition, post-operation. Any of the three may be omitted.

```fib
for (i: int4 = 0; i < len; i += 1) {
    libc::printf("%d\n", i)
}

// infinite loop
for (;;) {
    if cur as uint == 0 as uint {
        break
    }
    cur = cur.*.next
}

// while-style: only a condition
for (; v != 0 as uint4 ;) {
    v = v >> 1
}
```

## `break` / `continue`

Exit or skip the current iteration of the enclosing `for` loop.

```fib
for (;;) {
    if done { break }
    continue
}
```

## `ret`

Return from a function. The `ret` keyword is used (not `return`).

```fib
ret 0
ret a / b, a % b   // multiple return values
ret                 // bare return (void)
```

## `defer`

Schedule a statement to run when the enclosing function exits. Useful for cleanup paired with allocation.

```fib
fn main() int4 {
    head: *Node
    defer free_list(head)
    ...
    ret 0
}
```

## `switch` / `when`

Pattern matching on enums and tagged unions — see [Switch](switch.md).
