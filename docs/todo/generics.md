# TODO: improve generic UX

The issue at hand is that calling a generic function can be annoying because of
needing to write the generic types as arguments.
In simple examples this doesn't seem that bad (e.g. `cast(@float4, 0.3, @int4)`
but for longer and more cumbersome cases it can be pretty bad.

The proposed fix is to allow braket notation in the function (`[T]`) to allow 
generic type inference.

```fib
fn insertion_sort[T](arr: *T, len: @uint4) @void { ... }
```

A function declared with this syntax can be called like this:

```fib
insertion_sort(my_array, my_array.@len)
```

But it may also accept this syntax:

```fib
insertion_sort(@uint4, my_array, my_array.@len)
```
