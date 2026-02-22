# Function keyword

The `function` keyword is used in two places, in function declarations and in the function type.

## Function declarations

There are multiple syntaxes for function declarations.

1. Function declarations may or may not specify arguments.
2. Function declarations may or may not specify a return type. In case a return type is not specified, the inferred type is unit.
3. Function declarations may or may not specify a body. In case a body is not specified, the declaration is interpreted as a [forward declaration]().

**Syntax**:

```
function <identifier>();
function <identifier>(<argument-label> <argument-type>);
function <identifier>() <return type>;
function <identifier>(<argument-label> <argument-type>) <return-type>;
function <identifier>(<argument-label> <argument-type>, ...);
function <identifier>(<argument-label> <argument-type>, ...) <return-type>;
function <identifier>() { <body> }
function <identifier>(<argument-label> <argument-type>) { <body> }
function <identifier>() <return type> { <body> }
function <identifier>(<argument-label> <argument-type>) <return-type> { <body> }
function <identifier>(<argument-label> <argument-type>, ...) { <body> }
function <identifier>(<argument-label> <argument-type>, ...) <return-type> { <body> }
```

## Function type

This has not been fully implemented yet.

**Syntax**:

```
function(<argument-type>, ...) -> <return-type>
```
