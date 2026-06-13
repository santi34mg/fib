# Arrays

Fib supports fixed-size arrays. The size is part of the type.

## Declaration and literal

```fib
arr: @int4[8] = [3, 8, 5, 10, 2, 1, 6, 7]
```

The array literal `[ ... ]` is a comma-separated list of expressions.

## Indexing

Use the postfix `.[i]` to read or write an element:

```fib
arr.[i]              // read
arr.[j] = arr.[j-1]  // write
```

The same index syntax works on pointers (treating them as the start of an array):

```fib
fn print_arr(T: type, arr: *T, len: @int4) @void {
    for (i: @int4 = 0; i < len; i += 1) {
        libc::printf(" %d ", arr.[i])
    }
}
```

## Passing arrays

Take the address of a local array and pass it as a pointer:

```fib
arr: @int4[8] = [3, 8, 5, 10, 2, 1, 6, 7]
insertion_sort(@int4, arr.& as *@int4, 8)
```
