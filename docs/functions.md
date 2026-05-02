# Functions

Functions are introduced with `fn`.

## Basic syntax

```fib
fn name(param1, param2, ...) ReturnType {
    // body
}
```

The return type follows the parameter list, with no arrow. A function returning nothing uses `void` (or omits the type, depending on context).

```fib
fn main() int4 {
    ret 0
}

fn print_list(*Node head) void {
    ...
}
```

## Parameters

Each parameter is a `type name` (or `name type`) pair. Both orders appear in samples:

```fib
fn pack_ipv4(uint4 a, uint4 b, uint4 c, uint4 d) uint4 { ... }
fn fib(n int4) int4 { ... }
```

Pointer-typed parameters use the `*T` syntax:

```fib
fn push(*Node head, int4 val) *Node { ... }
```

## Multiple return values

A function can return a tuple of values. The return type is parenthesized.

```fib
fn divmod(int4 a, int4 b) (int4, int4) {
    ret a / b, a % b
}

fn main() int4 {
    q, r := divmod(17, 5)
    ret 0
}
```

## Forward declarations

Declare a signature without a body by ending with a semicolon:

```fib
fn fib(n int4) int4;

fn fib(n int4) int4 {
    if n <= 1 { ret n }
    ret fib(n - 1) + fib(n - 2)
}
```

## `extern` functions

Bind a function imported from a C library. No body is provided.

```fib
extern fn printf(string fmt, ...) int4
extern fn malloc(uint8 size) *void
extern fn free(*void ptr) void
```

## Variadic functions

A trailing `...` makes a function variadic. Variadic functions are most commonly `extern` (e.g. `printf`).

```fib
extern fn printf(string fmt, ...) int4
```

## Type parameters (generics)

A parameter declared with the `type` keyword takes a compile-time type as its argument:

```fib
fn insertion_sort(T type, arr *T, len int4) void { ... }

insertion_sort(int4, arr.& as *int4, 8)
```

See [Generics](generics.md) for details.
