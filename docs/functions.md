# Functions

Functions are introduced with `fn`.

## Basic syntax

```fib
fn name(param1: Type1, param2: Type2, ...) ReturnType {
    // body
}
```

The return type follows the parameter list, with no arrow. A function returning nothing uses `void`.

```fib
fn main() int4 {
    return 0
}

fn print_list(head: *Node) void {
    ...
}
```

## Parameters

Each parameter is written `name: type`, comma-separated:

```fib
fn pack_ipv4(a: uint4, b: uint4, c: uint4, d: uint4) uint4 { ... }
fn fib(n: int4) int4 { ... }
```

Pointer-typed parameters use the `*T` syntax:

```fib
fn push(head: *Node, val: int4) *Node { ... }
```

## Multiple return values

A function can return a tuple of values. The return type is parenthesized.

```fib
fn divmod(a: int4, b: int4) (int4, int4) {
    return a / b, a % b
}

fn main() int4 {
    q, r := divmod(17, 5)
    return 0
}
```

## Forward declarations

Declare a signature without a body by ending with a semicolon:

```fib
fn fib(n: int4) int4;

fn fib(n: int4) int4 {
    if n <= 1 { return n }
    return fib(n - 1) + fib(n - 2)
}
```

## `extern` functions

Bind a function imported from a C library. No body is provided.

```fib
extern fn printf(fmt: string, ...) int4
extern fn malloc(size: uint8) *void
extern fn free(ptr: *void) void
```

## Variadic functions

A trailing `...` makes a function variadic. Variadic functions are most commonly `extern` (e.g. `printf`).

```fib
extern fn printf(fmt: string, ...) int4
```

## Type parameters (generics)

A parameter declared with the `type` keyword takes a compile-time type as its argument:

```fib
fn insertion_sort(T: type, arr: *T, len: int4) void { ... }

insertion_sort(int4, arr.& as *int4, 8)
```

See [Generics](generics.md) for details.

## Function types (reserved)

The type expression `fn(Type1, Type2) -> ReturnType` is parsed for future first-class function values, but calling through a function-typed binding is not supported yet.
