# Functions

## Function Declarations

Functions are declared with the `fn` keyword:

```
fn name(param1 Type1, param2 Type2) ReturnType {
    body
}
```

The return type is required. Use `void` for functions that do not return a value.

**Examples**:

```
fn add(a sint32, b sint32) sint32 {
    return a + b
}

fn greet() void {
    printf("Hello!\n")
}
```

## Forward Declarations

A function can be declared without a body (forward declaration). This is used to declare functions before their definition or to reference external functions:

```
fn name(param1 Type1, param2 Type2) ReturnType
```

**Example**:

```
fn printf(fmt string, ...) sint32
```

## Return Statements

A `return` statement exits the current function and optionally returns a value:

```
return expr
return
```

A bare `return` is valid in `void` functions.

## Parameters

Parameters are listed as `name Type` pairs separated by commas. Parameters are passed by value.

```
fn clamp(value sint32, min sint32, max sint32) sint32 {
    if value < min { return min }
    if value > max { return max }
    return value
}
```
