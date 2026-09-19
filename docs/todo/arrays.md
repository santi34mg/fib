## TODO: have compiler supported `.@len` for arrays

The compiler needs the size (length) of the array already, so why not give that 
value as a builtin property.

```fib
my_arr: @int4[3] = [0, 1, 2];
my_arr.@len     // 3
```

Also, I need to verify if arrays are a valid type in function signatures.
Because this:

```fib
fn insertion_sort[T](arr: *T, N: @uint4) @void { ... }
```

could be written like this:

```fib
fn insertion_sort[T](arr: T[N: @uint4]) @void { ... }
```

This is a bit denser but seems richer semantically.

