# Functions

> **Verified parser behavior:** Function declarations, signatures, and calls parse.

## Declarations and calls

```fib
fn add(a: int32, b: int32) int32 {
    return a + b;
}
fn finish() void {
    return;
}
fn implicit_void() { }

result := add(1, 2);
```

Parameters are named `name: Type`; use comma-separated lists. A declaration's
return type follows `)` **without an arrow**. Omit it before `{` or `;` to record
`void`. Body statements require semicolons, but there is no extra semicolon
after the function's closing brace.

Function declarations are dispatched at file level. Nested function
declarations are not accepted by the ordinary block statement parser; type-valued
bindings use ordinary declaration syntax. A call
records any parsed callee expression plus arguments.

## Multiple return syntax

```fib
fn divmod(a: int32, b: int32) (int32, int32) {
    return a / b, a % b;
}
q, r := divmod(17, 5);
```

The signature contains a tuple type; `return` contains an expression list.
The multi-binding declaration preserves a single call initializer.
`q, r = divmod(17, 5);` is not parsed as multiple assignment.

## Bodyless and external declarations

```fib
fn declared(x: *char) bool;
extern fn printf(fmt: string, ...) int32;
extern fn malloc(size: usize) *void;
extern fn free(ptr: *void) void;
```

A semicolon records a bodyless signature. `extern` sets a declaration flag.
The parser also tolerates an external signature without `;` in some positions;
use explicit semicolons for clear examples.

## Variadic syntax

A final `...` is consumed in parameter lists and sets `is_variadic`. It must be
followed immediately by `)`, with no later parameter. The parser does not limit
variadic declarations to `extern`.

## Metatype parameters

```fib
fn identity(T: type, value: T) T {
    return value;
}
value := identity(int32, 42);
```

These forms parse; they do **not** implement generic specialization. The parser
records ordinary parameters and a builtin type value argument; the signature's
`generic_parameters` field remains empty. See [Generics](generics.md).

## Function types and call expressions

```fib
Callback: type = fn(int32) -> int32;
callback: fn(int32) -> int32;
callback(42);
```

Function type expressions use `->`, unlike declarations. A call through a named
binding parses. Function-value design is discussed in
[Name resolution](../name-resolution.md) and the [roadmap](../roadmap/).

Related follow-up is collected in this [GitHub issue #12](https://github.com/santi34mg/fib/issues/12).

Parser coverage: `test_function_declaration_with_params`,
`function_signatures_preserve_variadic_return_and_bodyless_forms`,
`test_function_call_expression`, and `composite_type_expressions_preserve_nesting`.
