# Structs

Structs are aggregate types with named fields.

## Declaring a struct type

Fields are written as `type name`, separated by commas. A trailing comma is allowed.

```fib
type Pool struct {
    uint8 start,
    uint8 block_size,
    uint8 capacity,
    uint8 used,
}
```

Self-referential structs use a pointer to the type being declared:

```fib
type Node = struct {
    int4 data,
    *Node next,
}
```

## Constructing a struct

Use the type name followed by braces with `field: value` pairs:

```fib
Pool { start: buf as uint8, block_size: 4 as uint8, capacity: 6 as uint8, used: 0 as uint8 }

Node { data: val, next: head }
```

The result can be assigned, returned, passed as an argument, or written through a pointer:

```fib
node.* = Node { data: val, next: head }
ret Pool { start: buf as uint8, ... }
```

## Field access

Use `.field` to read or write fields:

```fib
o.inner.value = 42
libc::printf("%d\n", pool.capacity)
```

For a pointer to a struct, dereference first:

```fib
pool.*.used               // read field through pointer
pool.*.used = pool.*.used + 1
```

## Address-of

Take a pointer to a local struct with the postfix `.&`:

```fib
pool: Pool = pool_create(...)
pool_alloc(pool.&)
```

## Nested structs

Structs can hold other struct values directly:

```fib
type Inner struct { uint8 value, }
type Outer struct { Inner inner, }

o: Outer = Outer { inner: Inner { value: 0 } }
o.inner.value = 42
```
