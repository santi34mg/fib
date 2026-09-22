# Arrays and slices

Fib supports fixed-size arrays. The size is part of the type.

## Declaration and literal

```fib
arr: @int4[8] = [3, 8, 5, 10, 2, 1, 6, 7]
```

The array literal `[ ... ]` is a comma-separated list of expressions.

## Length with `.@len`

Every array exposes its element count as a comptime property of type
`@usize`:

```fib
my_arr: @int4[3] = [0, 1, 2]
my_arr.@len     // 3 (@usize)
```

`.@len` also works on slices (see below), where it loads the runtime
length instead of a constant.

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

## Slices (`T[]`)

A slice is a runtime `(ptr, len)` view over elements of type `T`. It is
lowered as a two-field value struct, so it can be passed and returned by
value:

```fib
fn sum(s: @int4[]) @int4 {
    total: @int4 = 0
    for (i: @int4 = 0; i < s.@len as @int4; i += 1) {
        total += s.[i]
    }
    return total
}

fn main() @int4 {
    arr: @int4[4] = [10, 20, 30, 40]
    s: @int4[] = arr.[0..4]  // explicit `[a, b)` view over `arr`
    return sum(s)
}
```

Rules:

- Build a slice explicitly with `arr.[a..b]` (`[a, b)`, so `len = b - a`).
  It works on arrays and on slices (sub-slicing); bounds are unchecked.
  There is no implicit `T[N]` → `T[]` conversion: `s: @int4[] = arr`
  is an error.
- Range forms (all unchecked, `len = eff_end - start`):
  - `arr.[a..b]` → `[a, b)`; `arr.[a.=b]` → `[a, b]` (inclusive)
  - `arr.[a..]` → `[a, len)`; `arr.[..b]` → `[0, b)`
  - `arr.[.=b]` → `[0, b]` (inclusive); `arr.[..]` → full range.
  Omitted starts mean `0`, omitted ends mean `obj.@len`.
- `arr as T[]` is the explicit full-range view; array literals still
  coerce their elements, so `s: @int8[] = [1, 2]` works.
- `s.[i]` reads and `s.[i] = v` writes through the view — mutating a
  slice element mutates the underlying array.
- `s.@len` returns the runtime length as `@usize`.
- Slices compose with aliases (`type Ints @int4[]`) and generics
  (`fn first(T: type, s: T[]) T { return s.[0] }`).

See `samples/slices.fib` for a runnable program.
