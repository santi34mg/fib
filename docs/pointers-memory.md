# Pointers and Memory

Fib provides raw pointers and manual memory management. Allocation is performed by calling C library functions like `malloc` and `free`.

## Pointer types

`*T` is a raw pointer to `T`. `*@void` is a pointer with no element type, used for untyped buffers.

```fib
node: *Node
buf: *@void
val: *@int4
```

## `null`

The `null` literal is the null pointer. It can initialize or be compared against any pointer:

```fib
p: *@int4 = null
if p == null {
    libc::printf("empty\n")
}
n.* = Node { data: 42, next: null }
```

## Address-of and dereference

- `expr.&` — produces a pointer to `expr`
- `expr.*` — loads the value pointed to by `expr`

```fib
pool.&                   // *Pool from a Pool
cur.*                    // load Node from *Node
val.* = (i + 1) * 7      // store through pointer
```

## Pointer field access

When a pointer points to a struct, access fields by dereferencing first:

```fib
cur.*.next               // read field 'next' through *Node
pool.*.used = pool.*.used + 1
```

## Index access through pointers

`p.[i]` reads/writes the i-th element starting at `p`:

```fib
arr.[i]
arr.[j] = arr.[j - 1]
```

## Casting pointers

Use `as` to convert between pointer types or between a pointer and an integer:

```fib
buf as *Node
addr as *@void
cur as @uint8
```

## Manual allocation

The standard library exposes the C allocator via `std::libc`:

```fib
import std::libc

node: *Node = libc::malloc(16 as @uint8) as *Node
node.* = Node { data: val, next: head }
...
libc::free(cur as *@void)
```

## `defer` for cleanup

Pair allocations with `defer` to ensure they are released on every exit path:

```fib
pool: Pool = pool_create(4 as @uint8, 6 as @uint8)
defer pool_destroy(pool.&)
```

## Pointer variants (reserved)

The type system reserves additional pointer kinds beyond raw pointers — `unique &T`, `shared &T`, and `weak &T` — for future ownership/borrowing features. The currently usable pointer kind is the raw `*T`.
