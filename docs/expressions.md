# Expressions

## Arithmetic Operators

| Operator | Description    |
|----------|----------------|
| `+`      | Addition       |
| `-`      | Subtraction    |
| `*`      | Multiplication |
| `/`      | Division       |
| `%`      | Remainder      |

## Comparison Operators

| Operator | Description           |
|----------|-----------------------|
| `==`     | Equal                 |
| `!=`     | Not equal             |
| `>`      | Greater than          |
| `<`      | Less than             |
| `>=`     | Greater than or equal |
| `<=`     | Less than or equal    |

## Logical Operators

| Operator | Description |
|----------|-------------|
| `&&`     | Logical AND |
| `\|\|`   | Logical OR  |
| `!`      | Logical NOT |

## Bitwise Operators

| Operator | Description  |
|----------|--------------|
| `&`      | Bitwise AND  |
| `\|`     | Bitwise OR   |
| `^`      | Bitwise XOR  |
| `~`      | Bitwise NOT  |
| `<<`     | Left shift   |
| `>>`     | Right shift  |

## Assignment Operators

| Operator | Description             |
|----------|-------------------------|
| `=`      | Assign                  |
| `+=`     | Add and assign          |
| `-=`     | Subtract and assign     |
| `*=`     | Multiply and assign     |
| `/=`     | Divide and assign       |
| `%=`     | Remainder and assign    |

## Type Cast

The `as` keyword casts an expression to another type:

```
expr as Type
```

**Example**:

```
var float64 x = 3
var sint32 y = x as sint32
```

## Postfix Expressions

| Syntax            | Description                            |
|-------------------|----------------------------------------|
| `expr.field`      | Field access                           |
| `expr.*`          | Pointer dereference                    |
| `expr.&`          | Address-of (take a pointer)            |
| `expr.[index]`    | Array / pointer index access           |
| `expr(args)`      | Function call                          |

**Examples**:

```
point.x
ptr.*
value.&
arr.[0]
add(1, 2)
```

## Struct Construction

Construct a struct value by naming the type and providing field initializers:

```
TypeName { field1: expr1, field2: expr2 }
```

**Example**:

```
const Point p = Point { x: 1.0, y: 2.0 }
```

## Array Literals

An array literal is a comma-separated list of expressions in brackets:

```
[expr1, expr2, expr3]
```

**Example**:

```
const sint32[3] nums = [1, 2, 3]
```

## Unary Operators

| Operator | Description     |
|----------|-----------------|
| `-expr`  | Arithmetic negation |
| `!expr`  | Logical NOT     |
| `~expr`  | Bitwise NOT     |

## Operator Precedence

From highest to lowest precedence:

| Level | Operators                          |
|-------|------------------------------------|
| 7     | Postfix: `.field`, `.*`, `.&`, `.[i]`, `(args)` |
| 6     | Unary: `-`, `!`, `~`               |
| 5     | `*`, `/`, `%`                      |
| 4     | `+`, `-`                           |
| 3     | `<<`, `>>`                         |
| 2     | `<`, `>`, `<=`, `>=`, `==`, `!=`   |
| 1     | `&`, `^`, `\|`, `&&`, `\|\|`      |
| 0     | `as` (type cast)                   |

Use parentheses to override precedence.
