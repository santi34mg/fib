# Variables and Constants

## Variable Declarations

Variables are declared with the `var` keyword:

```
var Type name
var Type name = expr
```

If no initializer is given, the variable is zero-initialized.

**Examples**:

```
var sint32 count
var float64 ratio = 3.14
var bool active = true
```

## Constant Declarations

Constants are declared with the `const` keyword. The type annotation is optional when it can be inferred from the expression:

```
const Type name = expr
const name = expr
```

**Examples**:

```
const sint32 max_retries = 5
const name = "fiber"
```

Constants may not be reassigned after declaration.

## Type Declarations

Named types are also declared with `const type` (see [Types](types.md)):

```
const type Name = TypeExpression
```

## Zero Initialization

Variables declared without an initializer are zero-initialized:

- Integer types → `0`
- Float types → `0.0`
- `bool` → `false`
- Pointers → `null`
