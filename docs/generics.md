# Generics via Type Parameters

Fib supports lightweight generics by accepting a *type* as an ordinary parameter. The parameter is declared with the `type` keyword as its type, and the resulting binding can be used anywhere a type is expected within the function.

## Declaring a type parameter

```fib
fn insertion_sort(T: type, arr: *T, len: @int4) @void {
    for (i: @int4 = 1; i < len; i += 1) {
        for (j: @int4 = i; j > 0; j -= 1) {
            if arr.[j] < arr.[j - 1] {
                t: T = arr.[j]
                arr.[j] = arr.[j - 1]
                arr.[j - 1] = t
            }
        }
    }
}
```

Inside the function body, `T` is used in any type position: parameter types, locals, casts, etc.

## Calling a generic function

Pass the type as the first argument, just like any other value:

```fib
arr: @int4[8] = [3, 8, 5, 10, 2, 1, 6, 7]

insertion_sort(@int4, arr.& as *@int4, 8)
print_arr(@int4, arr.& as *@int4, 8)
```

## Notes

- Type parameters are ordinary positional arguments; they don't use a separate angle-bracket syntax.
- A type argument can be any built-in type (e.g. `@int4`) or a user-declared type alias.
