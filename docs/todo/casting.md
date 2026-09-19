# TODO: casting

The current casting syntax leads to ambiguity about the operation being done 
and leaves some semantics on the table.

```fib
expr as Type
```

The syntax leaves conversion up to the compiler while the user might want to 
perform some specific type of casting. 
This is a known issue in the C++ world where the C-like casting is mostly 
recommended against. 

For example, does `0.3 as @int` round the value down, up, truncates it or 
reinterpret the bytes?

The easiest alternative for this problem is adding builtins to the language the
same way as C++ has different types of casts.

The `cast` function can be implemented as a generic function.

```fib
fn cast(from: type, value: from, to: type) to { ... }
```

Calling would be:

```fib
cast(@float4, 0.3, @int)
```
