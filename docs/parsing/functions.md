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
fn declared(x: *uint8) bool;
callconv "C" fn puts(text: const [*:0]uint8) int32;
callconv "C" fn malloc(size: usize) *void;
callconv "C" fn free(ptr: *void) void;
```

A semicolon records a bodyless signature. `callconv "C"` selects the C calling
convention on declarations and function types. The removed `extern` spelling
produces a diagnostic directing callers to `callconv "C"`.

## Compile-time declarations and removed variadics

```fib
comp fn identity_type(T: type) type { return T; }
callback: comp fn(type) -> type;
```

`comp` is an explicit function declaration/type modifier, distinct from an
ordinary `fn`. Parsing records eligibility; bounded semantic type computation
requires a body returning `type`. Knowing an ordinary function's identity does
not authorize evaluation. Function type syntax retains `->`; declarations do not.
Variadic `...` parameter syntax has been removed and produces a diagnostic.

## Metatype parameters

```fib
fn identity(T: type, value: T) T {
    return value;
}
value := identity(int32, 42);
```

The parser records ordinary parameters and type-value arguments. Semantic analysis
specializes concrete instances and rechecks their bodies; those instances execute
through lowering and LLVM. See [Generics](generics.md).

## Function types and call expressions

```fib
comp Callback: type = fn(int32) int32;
callback: fn(int32) int32;
callback(42);
```

A call through a named binding parses. Function-value design is discussed in
[Name resolution](../name-resolution.md) and the [roadmap](../roadmap/).

Related follow-up is collected in this [GitHub issue #12](https://github.com/santi34mg/fib/issues/12).

Parser coverage: `test_function_declaration_with_params`,
`function_signatures_preserve_return_and_bodyless_forms`,
`test_function_call_expression`, and `composite_type_expressions_preserve_nesting`.
