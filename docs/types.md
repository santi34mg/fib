# Types

## Built-in Types

### Integer Types

| Type     | Description              |
|----------|--------------------------|
| `uint8`  | Unsigned 8-bit integer   |
| `uint16` | Unsigned 16-bit integer  |
| `uint32` | Unsigned 32-bit integer  |
| `uint64` | Unsigned 64-bit integer  |
| `sint8`  | Signed 8-bit integer     |
| `sint16` | Signed 16-bit integer    |
| `sint32` | Signed 32-bit integer    |
| `sint64` | Signed 64-bit integer    |

### Float Types

| Type      | Description               |
|-----------|---------------------------|
| `float32` | 32-bit floating point     |
| `float64` | 64-bit floating point     |

### Other Types

| Type     | Description                                      |
|----------|--------------------------------------------------|
| `bool`   | Boolean (`true` or `false`)                      |
| `char`   | Single character                                 |
| `string` | String value                                     |
| `void`   | No value (used as function return type)          |
| `never`  | Type of expressions that never return            |

## Type Declarations

A named type alias is declared with `const type`:

```
const type Name = TypeExpression
```

**Example**:

```
const type Score = sint32
```

## Struct Types

A struct is an aggregate type with named fields. Structs are declared using `const type` with a struct expression:

```
const type Name = struct {
    field1 Type1,
    field2 Type2,
}
```

**Example**:

```
const type Point = struct {
    x float32,
    y float32,
}
```

## Array Types

An array type is written as the element type followed by a size in brackets:

```
Type[size]
```

**Example**:

```
const type Buffer = uint8[256]
```

## Pointer Types

A raw pointer is written with a leading `*`:

```
*Type
```

**Example**:

```
var *sint32 p = x.&
```

See [Expressions](expressions.md) for address-of (`.&`) and dereference (`.*`) syntax.
