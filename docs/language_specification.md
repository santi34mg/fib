# Language specification

<!-- START doctoc generated TOC please keep comment here to allow auto update -->
<!-- DON'T EDIT THIS SECTION, INSTEAD RE-RUN doctoc TO UPDATE -->

- [Language specification](#language-specification)
  - [Introduction](#introduction)
  - [Notation](#notation)
  - [Program](#program)
  - [Lexical elements](#lexical-elements)
    - [Identifier](#identifier)
      - [Array literals](#array-literals)
      - [Range literals](#range-literals)
      - [Map literals](#map-literals)
      - [Struct literals](#struct-literals)
      - [Variant literal](#variant-literal)
      - [Unit literal](#unit-literal)
  - [Names and bindings](#names-and-bindings)
    - [Entity](#entity)
    - [Bindings](#bindings)
    - [Scope](#scope)
    - [Environment](#environment)
    - [Name resolution](#name-resolution)
    - [Modules](#modules)
      - [Module declarations](#module-declarations)
      - [Default module](#default-module)
      - [Module member declarations](#module-member-declarations)
      - [Module member references](#module-member-references)
      - [Submodules and nesting](#submodules-and-nesting)
      - [Name resolution in modules](#name-resolution-in-modules)
      - [Using external modules](#using-external-modules)
      - [Visibility and access control](#visibility-and-access-control)
  - [Semantics](#semantics)
    - [Values](#values)
      - [Atomic values](#atomic-values)
      - [Composite values](#composite-values)
    - [Evaluation](#evaluation)
  - [Expressions](#expressions)
    - [Primary expressions](#primary-expressions)
    - [Operators](#operators)
      - [Arithmetic operators](#arithmetic-operators)
      - [Compound assignment operators](#compound-assignment-operators)
      - [Comparison operators](#comparison-operators)
        - [Structural equality](#structural-equality)
        - [Strict type equality](#strict-type-equality)
        - [Function type equality](#function-type-equality)
        - [Ordering operators](#ordering-operators)
      - [Logical operators](#logical-operators)
      - [Bitwise operators](#bitwise-operators)
      - [Operator precedence and associativity](#operator-precedence-and-associativity)
    - [Block expressions](#block-expressions)
  - [Variables](#variables)
    - [Immutable variable declarations](#immutable-variable-declarations)
    - [Mutable variable declarations](#mutable-variable-declarations)
    - [Variable assignment](#variable-assignment)
    - [Type inference](#type-inference)
  - [Control flow](#control-flow)
    - [Conditional statements](#conditional-statements)
      - [If statement](#if-statement)
      - [If-else statement](#if-else-statement)
      - [If-else if chains](#if-else-if-chains)
      - [If expressions](#if-expressions)
    - [Iteration statements](#iteration-statements)
      - [For statement](#for-statement)
      - [Break statement](#break-statement)
      - [Continue statement](#continue-statement)
    - [Pattern matching](#pattern-matching)
      - [Match expressions](#match-expressions)
      - [Patterns](#patterns)
      - [Pattern matching on sum types](#pattern-matching-on-sum-types)
      - [Guards](#guards)
    - [Return statement](#return-statement)
    - [Destructuring](#destructuring)
      - [Positional product type destructuring](#positional-product-type-destructuring)
      - [Labeled products type destructuring](#labeled-products-type-destructuring)
      - [Array index access](#array-index-access)
  - [Memory model, ownership, and lifetime](#memory-model-ownership-and-lifetime)
    - [Lifetime rules](#lifetime-rules)
    - [Initialization](#initialization)
    - [Evaluation order](#evaluation-order)
    - [ABI and FFI](#abi-and-ffi)
  - [Undefined and implementation-defined behavior](#undefined-and-implementation-defined-behavior)
  - [Type system](#type-system)
    - [Types](#types)
    - [Generics and Code Generation](#generics-and-code-generation)
    - [Primitive types](#primitive-types)
    - [Structs](#structs)
    - [Variants](#variants)
    - [Array types](#array-types)
      - [Array indexing with ranges](#array-indexing-with-ranges)
      - [Array indexing with arrays](#array-indexing-with-arrays)
      - [Slice types](#slice-types)
    - [Map types](#map-types)
      - [Map access](#map-access)
      - [Map key requirements](#map-key-requirements)
      - [Missing key behavior](#missing-key-behavior)
      - [Nested maps](#nested-maps)
    - [Function types](#function-types)
    - [Type composition](#type-composition)
    - [Named types](#named-types)
    - [Pointers](#pointers)
      - [Safe pointer types](#safe-pointer-types)
        - [Unique pointers](#unique-pointers)
        - [Shared pointers](#shared-pointers)
        - [Weak pointers](#weak-pointers)
      - [Raw pointers](#raw-pointers)
        - [Pointer arithmetic](#pointer-arithmetic)
      - [Obtaining addresses](#obtaining-addresses)
      - [Dereferencing](#dereferencing)
      - [Null pointers](#null-pointers)
    - [Contracts](#contracts)
      - [Contract declarations](#contract-declarations)
      - [Contract implementations](#contract-implementations)
      - [Contract-typed parameters](#contract-typed-parameters)
      - [Monomorphization](#monomorphization)
    - [Errors](#errors)
      - [The Error contract](#the-error-contract)
      - [Defining custom errors](#defining-custom-errors)
      - [Using errors in functions](#using-errors-in-functions)
      - [Handling errors](#handling-errors)
      - [Functions accepting any error](#functions-accepting-any-error)
      - [Panic](#panic)
    - [Type casting](#type-casting)
      - [Raw casting](#raw-casting)
      - [Safe casting](#safe-casting)
    - [Dynamic type](#dynamic-type)
    - [Blob type](#blob-type)
    - [Never type](#never-type)
  - [Functions](#functions)
    - [Function declarations](#function-declarations)
    - [Forward declarations](#forward-declarations)
    - [Parameters](#parameters)
    - [Function application](#function-application)
    - [Recursion](#recursion)
    - [Anonymous functions](#anonymous-functions)
    - [Closures](#closures)
    - [Purity and side effects](#purity-and-side-effects)
  - [Memory management](#memory-management)
    - [Stack allocation](#stack-allocation)
    - [Heap allocation](#heap-allocation)
    - [Deallocation](#deallocation)
    - [Deferred deallocation](#deferred-deallocation)
    - [Arena allocation](#arena-allocation)
  - [Compiler Hints](#compiler-hints)
    - [Hint Syntax](#hint-syntax)
    - [Parameter Hint Semantics](#parameter-hint-semantics)
      - [Requirement Hints (Inline Placement)](#requirement-hints-inline-placement)
      - [Promise Hints (External Placement)](#promise-hints-external-placement)
      - [Combining Requirement and Promise Hints](#combining-requirement-and-promise-hints)
      - [Verification Rules](#verification-rules)
    - [Hint Placement Summary](#hint-placement-summary)
  - [Stackful coroutines](#stackful-coroutines)
  - [Code generation](#code-generation)

<!-- END doctoc generated TOC please keep comment here to allow auto update -->

<div class="page"/>

## Introduction

fib is a general purpose language built with data and systems engineering in mind. It is built around modularity and user control.

## Notation

The syntax notation in this document uses informal conventions based on common patterns. A formal grammar will be provided in future revisions.

## Program

A _program_ is a collection of one or more modules.
Program execution begins at a designated entry point.

## Lexical elements

### Identifier

An _identifier_ is a lexical token that represents a name in program text.
Identifier occurrences may refer to entities through bindings established by declarations.
The interpretation of identifier occurrences is governed by the language's rules for binding, scope, and name resolution.

**Regex**:

```
[A-Za-z_][A-Za-z0-9_]*
```

Identifiers must not match any reserved keyword.

#### Array literals

An array literal constructs an array value.

**Syntax**:

```
[<expression>, <expression>, ...]
```

**Examples**:

```
[1, 2, 3, 4, 5]
["a", "b", "c"]
[]  // Empty array (type must be inferable from context)
```

#### Range literals

A range literal constructs an array containing a sequence of consecutive integer values.
Range literals are syntactic sugar; the compiler lowers them to equivalent array literals at compile time.

**Syntax**:

```
(<start_inclusive>..<end_exclusive>)
```

The start value is inclusive and the end value is exclusive.
Both values must be integer expressions.
When the start or end is omitted in a slicing context, defaults are inferred from the array bounds.

**Examples**:

```
let range [6]int = (0..6);   // equivalent to [0, 1, 2, 3, 4, 5]
let range [3]int = (5..8);   // equivalent to [5, 6, 7]
```

Range literals may also be used to index arrays, producing a slice or selecting multiple elements.
See [Array indexing with ranges](#array-indexing-with-ranges) for details.

#### Map literals

A map literal constructs a map value, associating keys with values.

**Syntax**:

```
{ <key_expression> -> <value_expression>, <key_expression> -> <value_expression>, ... }
```

The `->` operator associates a key with a value, consistent with pattern matching syntax.
An empty map is written as `{}`.

**Examples**:

```
let weekdays string[]int = {
    "monday" -> 0,
    "tuesday" -> 1,
    "wednesday" -> 2
};

let empty_map int[]string = {};

// Nested maps
let nested string[](int[]bool) = {
    "flags" -> { 0 -> true, 1 -> false },
    "empty" -> {}
};
```

See [Map types](#map-types) for type syntax and access methods.

#### Struct literals

A struct literal constructs a value of struct type.

**Syntax**:

```
struct { <label> = <expression>, ... }
```

**Examples**:

```
struct { x = 10, y = 20 }
struct { name = "Alice", age = 30, active = true }
```

#### Variant literal

A variant literal constructs a value of variant type.

**Syntax**:

```
variant {<label> = <expression>}
```

**Examples**:

```
variant {Ok = 10}
variant {Left "hello"}
```

#### Unit literal

The unit literal represents the single value of the unit type.

**Syntax**:

```
()
```

<div class="page"/>

## Names and bindings

### Entity

An _entity_ is a denotable object in the language—such as a variable, function, type, or module—that may be associated with an identifier through a binding.

### Bindings

A binding associates an identifier with an entity within a given scope.

### Scope

A _scope_ is a region of program text in which a given set of bindings is in effect.
[Name resolution](#name-resolution) determines which binding an identifier occurrence refers to within a scope.

### Environment

An _environment_ is a mapping from identifiers to their associated bindings, used during [name resolution](#name-resolution) and [evaluation](#evaluation).

When a new scope is entered, a new environment is created that extends the enclosing environment with additional bindings.

### Name resolution

An identifier occurrence is either a _binding occurrence_, which introduces a new binding, or a _reference occurrence_, which refers to an existing binding.

Name resolution is the process of determining, for each reference occurrence of an identifier, the binding to which it refers based on scope.

### Modules

A _module_ is a program unit that defines a scope and an interface.

A module may contain bindings to variables, types, functions, coroutines and other modules.
A module's _interface_ is the set of public bindings that the module exports for external access.
The module interface determines which bindings defined in the module are accessible outside the module.

#### Module declarations

A _module declaration_ introduces a name for a module.
The module name must be a valid identifier.

Module declarations may only appear at the top level of a file.
There may only be one module declaration per file.
A module may not be declared in more than one file.

**Syntax**:

```
[public | private] module <module_identifier>[;]
```

Modules are private by default.
The visibility of the module defines the visibility of its bindings for external modules.

**Example**:

```
module cats;

public type Cat = ('name string * 'size int)
```

Bindings declared within a module's are locally scoped to that block.

#### Default module

If no module is explicitly declared, an implicit module named after the file name is declared.

#### Module member declarations

Entities declared within the file of a module declaration are declared within that module's scope.
If no module was declared, the default module is used instead.

#### Module member references

Bindings from other modules may be referenced using qualified identifiers.

**Syntax**:

```
<module_identifier>:<entity_identifier>
```

Name resolution for qualified identifiers locates the binding in the specified module's scope.

#### Submodules and nesting

A _submodule_ is a module (refered to as the child) that has been binded to another module (refered to as the parent).
This process is called _module nesting_.
A sibling is any entity that shares a common parent with a submodule.

To declare a submodule use the following syntax:

```
[public | private] module <parent-module>:<children-module>
```

The parent module may be a submodule as well.

#### Name resolution in modules

Within a module's scope, unqualified identifier occurrences are resolved by searching the local scope first.
If no binding is found in the local scope, name resolution searches enclosing scopes.

#### Using external modules

The `use` keyword brings another module's public bindings to the scope of the current module.
This simplifies the use of nested modules.

```
use Animals:Mammals:Pets:Cats:create_cat

function main() unit {
    let cat = create_cat()
}
```

Otherwise, to reference an entity from another module, a qualified identifier must be used.

```
function main() unit {
    let cat = Animals:Mammals:Pets:Cats:create_cat()
}
```

#### Visibility and access control

Modules may be declared as `public` or `private` (default).

Entities declared within a public module are public by default; entities in a private module are private.
Private entities are not accessible outside their module's scope.

The `private` and `public` keywords may be used to explicitly override the default visibility of an entity.

Access rules:

- Public entities are accessible from any scope.
- Private entities are accessible only within their module's scope.
- Attempting to access a private entity from outside its module causes a compile-time error.

<div class="page"/>

## Semantics

### Values

A _value_ is a runtime datum that may be stored in a variable, produced by evaluation, or passed as an argument.

Identifiers do not name values directly; they name entities (such as variables) that may hold values.

#### Atomic values

An _atomic value_ is an indivisible datum with no internal structure accessible to the program.
Atomic values cannot be decomposed into constituent parts.
An atomic value represents a single, self-contained unit of data.

#### Composite values

A _composite value_ is a datum composed of zero or more _component values_ organized according to a specific structural form.
Component values may themselves be atomic or composite.

Composite values may be decomposed into their constituents through [destructuring](#destructuring).

### Evaluation

_Evaluation_ is the process of computing a value from an expression under a given environment.

Evaluation produces values but does not introduce bindings.

<div class="page"/>

## Expressions

An _expression_ is a syntactic construct that denotes a value.
Expressions are evaluated to produce values.

### Primary expressions

Primary expressions are the basic building blocks of larger expressions:

- Literals
- Identifiers (variable references)
- Parenthesized expressions: `(<expression>)`
- Function applications: `<identifier>(<arguments>)`
- Field access: `<expression>.<identifier>` or `<expression>.<index>`

### Operators

Operators combine expressions to form compound expressions.

#### Arithmetic operators

Arithmetic operators operate on integer values and produce integer results.

| Operator    | Description        | Example |
| ----------- | ------------------ | ------- |
| `+`         | Addition           | `a + b` |
| `-`         | Subtraction        | `a - b` |
| `*`         | Multiplication     | `a * b` |
| `/`         | Integer division   | `a / b` |
| `%`         | Modulo (remainder) | `a % b` |
| `-` (unary) | Negation           | `-a`    |

Division by zero causes a runtime error or may be handled via error values.

#### Compound assignment operators

Compound assignment operators combine an arithmetic operation with assignment.

| Operator | Description         | Example  | Equivalent  |
| -------- | ------------------- | -------- | ----------- |
| `+=`     | Add and assign      | `a += b` | `a = a + b` |
| `-=`     | Subtract and assign | `a -= b` | `a = a - b` |
| `*=`     | Multiply and assign | `a *= b` | `a = a * b` |
| `/=`     | Divide and assign   | `a /= b` | `a = a / b` |
| `%=`     | Modulo and assign   | `a %= b` | `a = a % b` |

The left operand must be a variable.
The expression `a op= b` is equivalent to `a = a op b`, except that `a` is evaluated only once.

#### Comparison operators

Comparison operators compare two values and produce a boolean result.

| Operator | Description            | Example   |
| -------- | ---------------------- | --------- |
| `==`     | Structural equality    | `a == b`  |
| `!=`     | Structural inequality  | `a != b`  |
| `===`    | Strict type equality   | `a === b` |
| `!==`    | Strict type inequality | `a !== b` |
| `<`      | Less than              | `a < b`   |
| `>`      | Greater than           | `a > b`   |
| `<=`     | Less than or equal     | `a <= b`  |
| `>=`     | Greater than or equal  | `a >= b`  |

##### Structural equality

Structural equality compares values based on their runtime representation.
Two values are structurally equal if they have compatible types and their components are recursively equal.

Two types are _compatible_ for structural equality if:

- They are the same type.
- One is a named type and the other is its underlying type (e.g., `Age` and `int` where `type Age = int`).
- Both are named types with the same underlying type (e.g., `Age` and `Year` where both alias `int`).
- Both are product types with the same number of components, and corresponding components have compatible types.
- Both are sum types with the same variants, and corresponding variant types are compatible.
- Both are array types with compatible element types.

Named types are considered compatible with their underlying types for structural equality.

**Example**:

```
type Age = int;
let x int = 25;
let y Age = 25;
x == y  // true (same underlying value)
```

For composite types, structural equality compares components recursively:

```
type Age = int;
let p1 (string * int) = ("J", 20);
let p2 (string * Age) = ("J", 20);
p1 == p2  // true (structurally equivalent)
```

##### Strict type equality

Strict type equality requires that both the values and their complete type structures are identical.
Named types are distinguished from their underlying types and from other named types.

**Example**:

```
type Age = int;
type Year = int;

let a Age = 25;
let y Year = 25;
let i int = 25;

a === a   // true (same type, same value)
a === y   // false (Age !== Year)
a === i   // false (Age !== int)
a == i    // true (structurally equal)
```

For composite types, strict equality checks type identity recursively at each level:

```
type Age = int;
let p1 (string * int) = ("J", 20);
let p2 (string * Age) = ("J", 20);
let p3 (string * Age) = ("J", 20);

p1 === p2  // false (int !== Age at second component)
p2 === p3  // true (identical types)
p1 == p2   // true (structurally equal)
```

##### Function type equality

Function types do not support equality comparison.
Attempting to compare function values with `==`, `!=`, `===`, or `!==` causes a compile-time error.

##### Ordering operators

Ordering operators are defined for numeric and character types.

#### Logical operators

Logical operators operate on boolean values.

| Operator | Description | Example    |
| -------- | ----------- | ---------- |
| `&&`     | Logical AND | `a && b`   |
| `\|\|`   | Logical OR  | `a \|\| b` |
| `!`      | Logical NOT | `!a`       |

Logical AND and OR use short-circuit evaluation: the right operand is evaluated only if necessary to determine the result.

#### Bitwise operators

Bitwise operators operate on the binary representation of integer values.

| Operator | Description              | Example  |
| -------- | ------------------------ | -------- |
| `&`      | Bitwise AND              | `a & b`  |
| `\|`     | Bitwise OR               | `a \| b` |
| `^`      | Bitwise XOR              | `a ^ b`  |
| `~`      | Bitwise NOT              | `~a`     |
| `<<`     | Left shift               | `a << n` |
| `>>`     | Right shift (arithmetic) | `a >> n` |

#### Operator precedence and associativity

Operators are listed from highest to lowest precedence.
Operators on the same line have equal precedence.

| Precedence  | Operators                                          | Associativity |
| ----------- | -------------------------------------------------- | ------------- |
| 1 (highest) | `()` (grouping), function call, `.` (field access) | Left-to-right |
| 2           | `-` (unary), `!`, `~`                              | Right-to-left |
| 3           | `*`, `/`, `%`                                      | Left-to-right |
| 4           | `+`, `-`                                           | Left-to-right |
| 5           | `<<`, `>>`                                         | Left-to-right |
| 6           | `<`, `<=`, `>`, `>=`                               | Left-to-right |
| 7           | `==`, `!=`, `===`, `!==`                           | Left-to-right |
| 8           | `&`                                                | Left-to-right |
| 9           | `^`                                                | Left-to-right |
| 10          | `\|`                                               | Left-to-right |
| 11          | `&&`                                               | Left-to-right |
| 12          | `\|\|`                                             | Left-to-right |
| 13 (lowest) | `=`, `+=`, `-=`, `*=`, `/=`, `%=`                  | Right-to-left |

> **Note**: `++` and `--` are statements, not operators, and do not appear in expressions.

Parentheses may be used to override precedence (grouping).

### Block expressions

A block expression groups statements and expressions, producing the value of its final expression.

**Syntax**:

```
{
    <statements>
    <expression>
}
```

If the block ends with a statement rather than an expression, the block produces the unit value `()`.

**Example**:

```
let result int = {
    let temp = compute();
    temp + 1
};
```

<div class="page"/>

## Variables

<!--
This is very inspired by Go's definition of a variable. For further reading, please see: https://go.dev/ref/spec#Variables
-->

A _variable_ is a storage location that holds a _value_.
A variable is associated with an identifier through a binding established by a variable declaration.
The [_type_](#types) of a variable restricts the set of allowed values for that variable as well as the _operations_ allowed with that variable.

### Immutable variable declarations

A _variable declaration_ introduces a binding that associates an identifier with a variable.
Variables are declared using the `let` keyword.

**Syntax**:

```
let <identifier> [<type>][;]
let <identifier> [<type>] = <expression>[;]
```

A variable declaration may optionally include an initializer expression.
When an initializer is present, the expression is evaluated and the resulting value is stored in the variable.

### Mutable variable declarations

The language supports two types of mutability.
The first is binding mutability.
The second is value mutability.
Binding mutability refers to the mutation of the binding itself.
Value mutability refers to the mutation of the value being binded.

Bindings and values are immutable by default.
This means that a variable's binding cannot be reassigned or the underlying value be changed.

The `let mut` statement may be used to create a mutable binding.

```
let mut <identifier> [<type>][;]
let mut <identifier> [<type>] = <expression>[;]
```

For mutable values, the type may be marked as mutable.

### Variable assignment

An _assignment_ evaluates an expression and stores the resulting value in an existing variable.
The binding remains unchanged; only the variable's stored value is updated.

**Syntax**:

```
<identifier> = <expression>[;]
```

The identifier must refer to a variable binding in scope.

### Type inference

Types may be inferred by ommitting the type in a declaration.

```
let x = 5
// same as
let x int = 5
```

For cases where the type may need extra information, portions of the type may still be inferred using `_`.

```
let x = 5                       // int is inferred
let p unique &_ = addressof x   // unique &int is inferred
```

<div class="page"/>

## Control flow

Control flow statements determine the order in which statements are executed.

### Conditional statements

Conditional statements execute code based on boolean conditions.

#### If statement

**Syntax**:

```
if <condition> {
    <statements>
}
```

The condition must be an expression of type `bool`.
The body executes if the condition evaluates to `true`.

#### If-else statement

**Syntax**:

```
if <condition> {
    <statements>
} else {
    <statements>
}
```

The else branch executes if the condition evaluates to `false`.

#### If-else if chains

**Syntax**:

```
if <condition1> {
    <statements>
} else if <condition2> {
    <statements>
} else if <condition3> {
    <statements>
} else {
    <statements>
}
```

Conditions are evaluated in order; the first true condition determines which branch executes.
The final `else` is optional and executes if no condition is true.

#### If expressions

Conditional constructs may be used as expressions when all branches produce values of the same type.

**Syntax**:

```
let x int = if condition { 1 } else { 2 };
```

When used as an expression, the `else` branch is required.

### Iteration statements

Iteration statements execute code repeatedly.

#### For statement

The `for` statement provides general-purpose iteration.

**Syntax (indefinite loop)**:

```
for {
    <statements>
}
```

This form loops indefinitely until terminated by `break` or `return`.

**Syntax (condition-controlled loop)**:

```
for <condition> {
    <statements>
}
```

The body executes repeatedly while the condition evaluates to `true`.
The condition is evaluated before each iteration.

**Syntax (C-style loop)**:

```
for <init>; <condition>; <post> {
    <statements>
}
```

The `init` statement executes once before the loop.
The `condition` is evaluated before each iteration; the loop terminates when it becomes `false`.
The `post` statement executes after each iteration.

**Examples**:

```
// Indefinite loop
for {
    if should_stop() { break; }
    process();
}

// Condition-controlled loop
let i int = 0;
for i < 10 {
    print(i);
    i = i + 1;
}

// C-style loop
for let i int = 0; i < 10; i++ {
    print(i);
}
```

#### Break statement

The `break` statement terminates the innermost enclosing loop.

**Syntax**:

```
break[;]
```

#### Continue statement

The `continue` statement skips the remainder of the current iteration and proceeds to the next iteration of the innermost enclosing loop.

**Syntax**:

```
continue[;]
```

### Pattern matching

#### Match expressions

The `match` expression inspects a value and executes code based on its structure.
Pattern matching is exhaustive: all possible cases must be handled.

All arms of a match expression must produce values of the same type.
The value of the match expression is the value produced by the matching arm.

**Syntax**:

```
match <expression> {
  <pattern> -> <expression>
  <pattern> -> <expression>
...
}
```

Each arm consists of a pattern and an expression separated by `->`.

#### Patterns

Patterns describe the structure of values to match against.

| Pattern                        | Description                       | Example                 |
| ------------------------------ | --------------------------------- | ----------------------- |
| `_`                            | Wildcard, matches any value       | `_`                     |
| `<identifier>`                 | Binds the matched value to a name | `x`                     |
| `<literal>`                    | Matches a specific literal value  | `42`, `"hello"`, `true` |
| `(<pattern>, <pattern>, ...)`  | Matches tuples                    | `(x, y)`                |
| `{ <label> = <pattern>, ... }` | Matches labeled products          | `{ x = a, y = b }`      |
| `'<variant> <pattern>`         | Matches a sum type variant        | `'Some x`               |

#### Pattern matching on sum types

Pattern matching is the primary mechanism for working with sum types.
Variant patterns use the `'` prefix matching the variant label.

**Example**:

```
type Option = 'None + 'Some int;

let value Option = 'Some 42;

let result int = match value {
  'None -> 0
  'Some x -> x
};
```

#### Guards

Patterns may include guards which are additional boolean conditions using the `when` keyword.

**Syntax**:

```
  <pattern> when <condition> -> <expression>
```

**Example**:

```
match x {
  n when n < 0 -> "negative"
  n when n > 0 -> "positive"
  _ -> "zero"
}
```

### Return statement

The `return` statement exits the current function and optionally provides a return value.

**Syntax**:

```
return[;]
return <expression>[;]
```

A `return` without an expression returns the unit value `()`.
The expression type must match the function's declared return type.

**Examples**:

```
function early_exit(x int) int {
    if x < 0 {
        return 0;
    }
    return x * 2;
}

function no_return_value() unit {
    print("done");
    return;
}
```

### Destructuring

Composite values can be _destructured_ to access component values within their structure.

#### Positional product type destructuring

Positional product types or tuples may be destructured using dot notation followed by a numeric index.
Indices are zero-based; the first element has index `0`.

**Syntax**:

```
<tuple_name>.<index>
```

**Example**:

```
let point (int * int) = (1, 2);
let x int = point.0;
let y int = point.1;
```

Multiple components of a tuple can be extracted simultaneously using tuple destructuring assignment.

**Syntax**:

```
let (<variable_name> [<variable_type>], ...) = <tuple_name>
```

If the variable type is not specified, the type will be inferred.

**Example**:

```
let point (int * int) = (1, 2);
let (x int, y int) = point;
```

#### Labeled products type destructuring

Labeled product type destructuring or structs can be destructured using dot notation followed by the field label.

**Syntax**:

```
<struct_name>.<field_label>
```

**Example**:

```
let person ('name string * 'age int) = { name = "John Smith", age = 25 };
let name string = person.name;
```

Labeled product type destructuring is resolved at compile-time with zero-cost abstractions.
The compiler statically resolves field accesses to direct memory offsets, producing the same machine code as manually accessing tuple indices.

#### Array index access

Array elements are accessed using dot notation followed by a numeric index.
Indices are zero-based; the first element has index `0`.

**Syntax**:

```
<array_name>.<index>
```

**Example**:

```
let arr []int = [1, 2, 3, 4, 5];
let third int = arr.2;
```

<div class="page"/>

## Memory model, ownership, and lifetime

fib uses a pragmatic memory and value model:

- All variables and heap allocations must be initialized before use.
- By default, values (ints, structs, etc.) use copy semantics: assignment and passing by value create independent copies.
- Move semantics (where assignment or passing invalidates the source) are only enabled for types or variables explicitly marked with a compiler hint (`@drop_if_moved`). Unique pointers (`unique &T`) have this hint by default via the standard library.
- Shared pointers (`shared &T`) use reference counting for memory management. Multiple shared pointers may reference the same memory; memory is freed when the last reference is dropped.
- Weak pointers (`weak &T`) do not affect reference counts and must be upgraded to shared pointers before use.

### Lifetime rules

- Stack-allocated variables live until the end of their scope.
- Heap-allocated values live until their last unique owner is dropped (for unique pointers) or reference count reaches zero (for shared pointers).
- Borrowed references (if supported) must not outlive their referent.

Violations of lifetime rules (use-after-free, double-free, dangling pointer) are compile-time errors where detectable, and always forbidden.

### Initialization

Variables may be initalized while being declared.
Uninitialized variables cannot be read from. Doing so results in a compile time error.

### Evaluation order

Unless otherwise specified, evaluation order is left-to-right for all function arguments and subexpressions. Side effects occur in evaluation order. Any deviation is implementation-defined and must be documented.

### ABI and FFI

The Application Binary Interface (ABI) is implementation-defined.

> TODO: Interfacing with foreign code (FFI) has not been specified yet by this language specification.

## Undefined and implementation-defined behavior

Undefined behavior is any program action for which this specification imposes no requirements. Implementations may behave unpredictably in such cases. Implementation-defined behavior must be documented by the implementation and should be minimized.

## Type system

fib has static typing with type inference at compile time. All types and type conversions are checked at compile time unless explicitly marked as runtime-checked. Type errors are always compile-time errors.

### Types

A _type_ defines the set of values a variable may hold and the operations that may be performed on those values.

fib supports several type constructors that allow building complex types from simpler ones.

### Generics and Code Generation

fib does not support generic types or generic contracts. All types, functions, and data structures must be defined with concrete types. Patterns that would typically use generics in other languages (such as parameterized data structures or interfaces) should be implemented using code generation or explicit code duplication.

This design choice ensures predictable performance and simplifies the type system. For reusable data structure patterns (such as lists, maps, or option types), developers are encouraged to use code generation tools or macros to produce the required concrete types and functions for each use case.

There is no syntax for `type List T` or `contract Poll T` in the language.

### Primitive types

Primitive types are the fundamental, built-in types of the language.

| Type     | Description            | Examples           |
| -------- | ---------------------- | ------------------ |
| `int`    | Signed integer numbers | `42`, `-1`, `0xFF` |
| `float`  | Floating-point numbers | `0.0f`, `3.14f`    |
| `char`   | Unicode code point     | `'a'`, `'\n'`      |
| `bool`   | Boolean truth value    | `true`, `false`    |
| `string` | Sequence of characters | `"hello"`          |
| `unit`   | Single-valued type     | `()`               |

The `string` type is a primitive type representing immutable sequences of Unicode characters.
Strings have value semantics: assignment and parameter passing copy the string value.

The `unit` type has exactly one value, written `()`.
It is used for functions that perform side effects without producing a meaningful result.

The `unit` type is zero-sized: values of type `unit` occupy no memory at runtime.
The compiler erases `unit` values during code generation, ensuring no runtime overhead for their use in parameters, return values, or composite types.

### Structs

A _struct_ represents a composite value containing multiple components.
Struct types are constructed using the `struct` keyword

**Syntax**:

```
struct {<field-name> <field-type> [= <default-expression>], ...}[;]
```

**Examples**:

```
struct {x int = 0, y int = 0}
struct {name string, age int, has_pets bool = false};
```

**Construction**:

Structs are constructed using a struct literal.

Individual members are accessed using dot notation:

```
let p struct {x int, y int} = struct {x int = 1, y int = 2};
let x_coord int = p.x;
```

> Note: notation seems redundant but it get easier by leveraging named types.

### Variants

A _variant_ represents a value that can be one of several possible types.
Variants are constructed using the `variant` keyword.

**Syntax**:

```
variant {<variant-name> <variant-type> [= <default-expression>], ...}[;]
```

Variants may carry a payload type or be unit variants (carrying no data).
A unit variant has no payload and is written with the unit type following the label.

**Examples**:

```
// Option type with a unit variant and a payload variant
variant {Some int, None unit};

// Result type for error handling
variant {Ok int, Err string};

// Either type with two payload variants
variant {Left string, Right int}

// Status with multiple unit variants
variant {Pending unit, Running unit, Completed unit, Failed string};
```

A variant value at runtime holds exactly one of the variant types.
Variants enable type-safe representation of alternatives and optional values.

**Construction**:

Variant values are constructed using the `variant` literal.

**Examples**:

```
let opt variant {Some int, None unit} = variant {Some = 42};
let none variant {Some int, None unit} = variant {None = ()};
let result variant {Ok int, Err string} = variant {Ok = 100};
let err variant {Ok int, Err string} = variant {Err = "something went wrong"};
```

The variant literal label must match one of the variant type labels.

> Note: notation seems redundant but it get easier by leveraging named types.

### Array types

An _array type_ represents a sequence of elements of the same type.

**Syntax**:

```
[<size>]<element_type>   // Fixed-size array
```

Arrays have a compile-time constant length specified in the type.

**Examples**:

```
let numbers []int = [1, 2, 3, 4, 5];
let buffer [1024]char;
let matrix [][3]int = [[1, 2, 3], [4, 5, 6]];
```

Array elements are accessed using dot notation with a numeric index (see [Array index access](#array-index-access)).
Indices are zero-based.

Bounds checking is performed statically at compile time when possible.
When static verification is not possible, runtime bounds checking is enabled by default.
Runtime bounds checking may be disabled using the [`@no_bounds_check`](#bounds-checking-hints) compiler hint.
Accessing an index outside the array bounds causes a panic.

#### Array indexing with ranges

Arrays may be indexed using range expressions to produce a slice.
A range expression specifies a contiguous subsequence of array elements.

**Syntax**:

```
<array_name>.(<start>..<end>)   // Elements from start (inclusive) to end (exclusive)
<array_name>.(..end)            // Elements from beginning to end (exclusive)
<array_name>.(start..)          // Elements from start (inclusive) to end of array
<array_name>.(..)               // All elements (entire array as slice)
```

**Examples**:

```
let arr [6]int = [0, 1, 2, 3, 4, 5];

let slice1 []int = arr.(0..3);  // [0, 1, 2]
let slice2 []int = arr.(..3);   // [0, 1, 2]
let slice3 []int = arr.(2..);   // [2, 3, 4, 5]
let slice4 []int = arr.(..);    // [0, 1, 2, 3, 4, 5]
```

#### Array indexing with arrays

Arrays may also be indexed using another array to select multiple elements at specified indices.

**Syntax**:

```
<array_name>.[<index_array>]
```

**Example**:

```
let data [6]int = [10, 20, 30, 40, 50, 60];
let indices [3]int = [0, 2, 4];

let selected [3]int = data.[indices];  // [10, 30, 50]
```

The result is an array containing the elements at the specified indices.
All indices must be within bounds; out-of-bounds indices cause a panic.

#### Slice types

A _slice_ is a view into a contiguous region of memory.
Slices provide a way to reference a portion of an array without copying the underlying data.

**Syntax**:

```
[]<element_type>
```

A slice is represented internally as a pointer to the first element and a length.
The equivalent structure of `[]T` is `('ptr &T * 'len int)`, where `T` represents any concrete element type.

**Examples**:

```
let arr [5]int = [1, 2, 3, 4, 5];

let slice []int = arr.(0..3);   // slice of elements at indices 0, 1, 2
                                // slice.len == 3, contents: [1, 2, 3]

let full []int = arr.(..);      // slice of entire array
                                // full.len == 5
```

Slices are created using dot notation followed by a range expression.
The slice references the original array's memory; modifications through the slice affect the original array.

**Slice properties**:

```
let s []int = arr.(1..4);
let length int = s.len;         // 3
let first int = s.0;            // element at index 0 of slice (arr.1)
```

Slice bounds are checked at runtime by default.
Accessing an index outside the slice bounds causes a panic.

### Map types

A _map_ is an associative data structure that maps keys to values.
Maps provide efficient lookup of values by their associated keys.

**Syntax**:

```
<key_type>[]<value_type>
```

The key type appears before `[]` and the value type appears after.
This syntax is distinct from slice types, which have no type before the brackets.

**Examples**:

```
let weekdays string[]int = {
    "monday" -> 0,
    "tuesday" -> 1,
    "wednesday" -> 2
};

let scores int[]string = {
    100 -> "perfect",
    0 -> "zero"
};

let empty string[]int = {};
```

##### Map access

Map values are accessed using the `get` method or the equivalent `map` method on the key.

**Syntax**:

```
<map_name>.get(<key_expression>)
<key_expression>.map(<map_name>)
```

Both forms are equivalent; the second form allows method chaining from the key.

**Examples**:

```
let days string[]int = { "mon" -> 0, "tue" -> 1 };

let day1 int = days.get("mon");     // 0
let day2 int = "tue".map(days);     // 1
```

##### Map key requirements

Key types must implement the `Hash * Eq` contracts.
This requirement enables O(1) average-case lookup performance.

##### Missing key behavior

Accessing a key that does not exist in the map causes a panic.
If the compiler can statically determine that a key is missing, it reports a compile-time error.

```
let m string[]int = { "a" -> 1 };

m.get("a");       // Valid: returns 1
m.get("b");       // Compile-time error: key "b" not in map
m.get(some_var);  // Runtime panic if some_var is not a key
```

Uninitialized maps cannot be accessed.
Attempting to access an uninitialized map causes a compile-time error if detectable, otherwise a runtime panic.

```
let m string[]int;    // Uninitialized
m.get("key");         // Compile-time error: map is uninitialized
```

##### Nested maps

Maps may be nested to create multi-level associations.

**Example**:

```
let config string[](int[]bool) = {
    "features" -> { 0 -> true, 1 -> false },
    "flags" -> {}
};

let feature_enabled bool = config.get("features").get(0);  // true
config.get("flags").get(0);  // Compile-time error: empty map
```

### Function types

A _function type_ represents a callable entity that accepts parameters and returns a value.
Function types are constructed using arrow syntax.

**Syntax**:

```
// Arguments
(<argument-types>) -> <return_type>
// No arguments
() -> <type>
```

**Examples**:

```
let transform (string) -> int;
let combine (int, int) -> int;
let produce () -> string;
let process (struct {x int, y int} -> variant {a bool, b string});
```

Function types are first-class, meaning they can be stored in variables, passed as arguments, and returned from functions.

### Type composition

Type constructors may be composed to create complex types:

```
let complex struct {x int, y string} -> variant {a bool, b int};
let handler ((string) -> int) -> string;
```

Operator precedence for type constructors (highest to lowest):

1. `->`
2. `struct`
3. `variant`

Parentheses may be used to override precedence.

### Named types

A _named type_ declaration introduces a name bound to a type expression.
Named types are declared using the `type` keyword.

**Syntax**:

```
type <identifier> = <type-expression>[;]
```

**Examples**:

```
type Point = struct {x int, y int};
type Result = variant {Ok int, Err string};
type Transform = (string) -> int;
```

Named types provide clarity and reusability for complex type expressions.
A named type and its underlying type expression are structurally equivalent and may be used interchangeably.

### Pointers

A _pointer_ is a value that holds the memory address of another value.
Pointers enable indirect access to data and are essential for dynamic memory management and building complex data structures.

fib provides two categories of pointers:

- **Safe pointers** (`unique &T`, `shared &T`, `weak &T`): The default; provide automatic memory management and compile-time safety guarantees.
- **Raw pointers** (`&T`): Low-level pointers with manual memory management; require `@unsafe` context.

#### Safe pointer types

Safe pointers are the default pointer types in fib.
They provide automatic memory management and prevent common errors such as null dereference, use-after-free, and double-free.

##### Unique pointers

A `unique &T` pointer represents exclusive ownership of a heap-allocated value.
Exactly one unique pointer owns the memory at any time.

**Syntax**:

```
unique &<type>
```

**Semantics**:

- When a unique pointer goes out of scope, the memory is automatically freed.
- Assignment transfers ownership (move semantics); the source pointer becomes invalid.
- Using a moved pointer is a compile-time error.

**Example**:

```
let x int = 5;
let p unique &int = addressof x;
deref p = 42;

let q unique &int = p;   // Ownership moves from p to q
// p is now invalid

print(deref q);          // Valid: prints 42
print(deref p);          // Compile-time error: p has been moved
```

##### Shared pointers

A `shared &T` pointer represents shared ownership of a heap-allocated value through reference counting.
Multiple shared pointers may reference the same memory.

**Syntax**:

```
shared &<type>
```

**Semantics**:

- Assignment creates a new reference and increments the reference count.
- When a shared pointer goes out of scope, the reference count is decremented.
- Memory is freed when the reference count reaches zero.

**Example**:

```
let x int = 5;
let p shared &int = addressof x;
deref p = 42;

let q shared &int = p;   // Reference count is now 2
let r shared &int = q;   // Reference count is now 3

// When p, q, and r all go out of scope, memory is freed
```

##### Weak pointers

A `weak &T` pointer is a non-owning reference to memory managed by shared pointers.
Weak pointers do not affect the reference count and may become invalid if all shared pointers are released.

**Syntax**:

```
weak &<type>
```

Weak pointers must be upgraded to shared pointers before dereferencing.
The upgrade operation is provided by the standard library and returns an option type indicating whether the referent still exists.

**Example**:

```
let x int = 5;
let p shared &int = addressof x;
deref p = 42;

let w weak &int = Weak:from(p);   // Create weak pointer (std library)

// Later, to use the weak pointer:
match Weak:upgrade(w) {
  'Some s -> print(deref s)
  'None -> print("referent has been freed")
}
```

#### Raw pointers

Raw pointers (`&T`) provide direct memory access without automatic management.
Raw pointers require the `@unsafe` hint and place full responsibility for memory safety on the developer.

**Syntax**:

```
&<type>
```

**Example**:

```
@unsafe {
    let x int = 5;
    let p &int = addressof x;
    print(deref p);          // prints 5

    deref p = 10;            // Write through pointer
    print(x);                // prints 10
}
```

##### Pointer arithmetic

Raw pointers support arithmetic operations for navigating contiguous memory.
Pointer arithmetic is allowed by default within `@unsafe` blocks.

**Example**:

```
@unsafe {
    alloc buffer &int, 10 = ZeroArray:new(10);

    let first &int = buffer;
    let second &int = buffer + 1;    // Points to second element
    let fifth &int = buffer + 4;     // Points to fifth element

    deref first = 100;
    deref second = 200;
}
```

Here the `ZeroArray` module from the standard library is being used.

#### Obtaining addresses

The `addressof` operator obtains the memory address of a variable.

**Syntax**:

```
addressof <variable_name>
```

The result type depends on the variable type being addressed.
For raw pointer usage, the result is `&T`.

**Example**:

```
let x int = 5;
let p unique &int = addressof x;   // Unique pointer to x
```

#### Dereferencing

_Dereferencing_ accesses the value at the address held by a pointer.
The `deref` operator is used for all pointer types.

**Syntax**:

```
deref <pointer_expression>
```

Dereferencing may be used to read or write the pointed-to value.

**Example**:

```
let x int = 5;
let p unique &int, 1 = addressof x;
deref p = 42;           // Write through pointer
let value int = deref p; // Read through pointer
```

TODO: verify if this statement is coherent
For safe pointers, dereferencing is always valid.
For raw pointers, dereferencing may lead to a panic.

#### Null pointers

A _null pointer_ is a pointer that does not reference any valid memory location.

**Creating null pointers**:

```
@unsafe {
    let p1 &int = null;
}
let p2 unique &int = null;
let p3 shared &int = null;
let p4 weak   &int = null;
```

**Null checks**:

Null pointers may be compared using equality operators.

```
@unsafe {
    let p &int = get_pointer();

    if p == null {
        print("pointer is null");
    } else {
        print(deref p);
    }
}
```

The compiler will try to determine if a pointer is null at the time of dereference.
If the compiler can statically determine that a null dereference is going to happen it will throw a compile time error.
Dereferencing a null pointer at runtime causes a panic.

```
@unsafe {
    let p &int = null;
    deref p;            // compile time error
}
```

This behavior applies to unique, shared and weak pointers as well.

### Contracts

A _contract_ defines a set of method requirements that types may implement.
Contracts enable polymorphic functions that accept any type satisfying the contract's requirements.

Contracts provide compile-time polymorphism through monomorphization.
When a function accepts a contract-typed parameter, the compiler generates specialized versions of that function for each concrete type used at call sites.

#### Contract declarations

A contract declaration introduces a named contract and specifies the method signatures that implementing types must provide.

**Syntax**:

```
contract <identifier> {
    function <method_name>(<parameters>) <return_type>;
    ...
}
```

Contract methods must include `self` as the first parameter if they operate on the implementing type's value.

**Example**:

```
contract Comparable {
    function compare(self, other Self) int;
}

contract Serializable {
    function to_bytes(self) []byte;
    function from_bytes(bytes []byte) Self;
}
```

The `Self` type refers to the type implementing the contract.

#### Contract implementations

Named types may implement one or more contracts.
Contract implementations are specified in an `impl` block after the type expresion.

**Syntax**:

```
<type-expression> impl <contract-name> {
    <implementations>
}
```

Each implementation includes the method definition.

**Example**:

```
contract Building {
    function get_address(self) string;
}
type House = struct {
    number_of_doors int,
    number_of_windows int,
    address string,
} impl Building {
    function get_address(self) string {
        return self.address;
    }
};
```

Multiple contract implementations may be specified.

All methods declared in a contract must be implemented.
Failing to implement all required methods causes a compile-time error.

#### Contract-typed parameters

Functions may accept parameters typed as contracts.
A contract-typed parameter accepts any type that implements the specified contract.

**Syntax**:

```
function <function_name>(<parameter_name> <contract_name>) <return_type> {
    <statements>
}
```

**Example**:

```
function calculate_area(shape Shape) int {
    return shape.area();
}
```

The function `calculate_area` accepts any type that implements the `Shape` contract.

Type algebra can be performed between contracts. The `*` operator means that both
contracts are required for that type while the `+` means that either contract
suffices.
Functions using contract-typed parameters with contracts composed of the `+` operator cannot use a method that is only provided by one of the underlying contracts.

#### Monomorphization

Contract-based polymorphism is resolved at compile-time through monomorphization.
When a function with contract-typed parameters is called with different concrete types, the compiler generates specialized versions of the function for each type.

**Example**:

```
let rect Rectangle = { width = 10, height = 20 };
let circ Circle = { radius = 5 };

calculate_area(rect);  // Generates calculate_area_Rectangle
calculate_area(circ);  // Generates calculate_area_Circle
```

Each specialized version has zero runtime overhead compared to a hand-written function for that specific type.
Method calls through contracts are statically resolved and may be inlined by the compiler.

This approach aligns with fib's zero-cost abstractions principle: generic code compiles to efficient, specialized machine code with no runtime polymorphism overhead.

### Errors

An _error_ is a value that represents that something in the usual or expected flow of the program has failed.
Errors are values and can be treated as any other of the language values.

#### The Error contract

The language defines an `Error` contract that types can sign to be treated as error types.
This contract-based approach allows developers to define their own custom error types.

**Contract definition**:

```
contract Error {
    function get_error_id(self) int;
    function get_error_message(self) string;
}
```

The `get_error_id` method returns an integer identifier for the error.
The `get_error_message` method returns a human-readable description of the error.

#### Defining custom errors

To define a custom error, create a named type that signs the `Error` contract:

```
type DivisionByZeroError = (
    'dividend int
    ;
    Error {
        function get_error_id(self) int {
            return 1;
        }
        function get_error_message(self) string {
            return "division by zero";
        }
    }
)

type FileNotFoundError = (
    'path string
    ;
    Error {
        function get_error_id(self) int {
            return 2;
        }
        function get_error_message(self) string {
            return "file not found: " + self.path;
        }
    }
)
```

#### Using errors in functions

Functions that may fail return a sum type containing either the success value or an error type.
Define a result type with labeled variants for clarity:

```
type DivideResult = 'Ok int + 'Err DivisionByZeroError;

function divide(a int, b int) DivideResult {
    if b == 0 {
        return 'Err DivisionByZeroError { dividend = a };
    }
    return 'Ok (a / b);
}
```

#### Handling errors

Errors may be handled using pattern matching on the labeled variants:

```
function main() unit {
    let result DivideResult = divide(10, 0);

    match result {
      'Err err -> print(err.get_error_message())
      'Ok value -> print(value)
    }
}
```

#### Functions accepting any error

Since `Error` is a contract, functions can accept any type that signs the contract:

```
function log_error(err Error) unit {
    print("Error " + err.get_error_id() + ": " + err.get_error_message());
}
```

#### Panic

The program can be abruptly interrupted and stopped through the use of the _panic_ keyword.

**Example**:

```
function divide_panic(a int, b int) int {
    if b == 0 {
        print("division by zero");
        panic
    }
    return a / b;
}
```

<div class="page"/>

### Type casting

Type casting allows converting a value from one type to another, either by reinterpreting the underlying bits (raw casting) or by using a safe conversion (safe casting).

#### Raw casting

`@raw_cast` may be used to reinterpret the data into another type.
Raw casting only changes the types; it does not perform any operation on the underlying value.
Raw casting requires the two types to be of the same size (e.g., you cannot cast from `int` to `bool` because the former is 4 bytes and the latter is 1 byte).

**Syntax**:

```
@raw_cast <<variable-initialization> | <variable-assignment>>
```

**Example**:

```
@raw_cast let c [4]char = 1752132705;
// ['h', 'o', 'l', 'a']
```

> Note: raw casting is not recommended for general use.

#### Safe casting

Safe casting is an abstraction over raw casting. Use the standard library to safely cast between types.

**Examples**:

```
let x int = 3.14f.cast_int();
// same as:
let x int = cast_int(3.14f)
```

### Dynamic type

`dynamic` is a type boundary marker.

`dynamic` types are constructed using a compile-time type synthesis: `(dynamic) <expression>`.

Each `(dynamic) <expression>` introduces a new, compiler generated concrete type whose structure is equal to the static type of `<expression>`.
If `<expression>` had type `T`, then `(dynamic) <expression>` will have a type denoted by `D_T` that is structurally equal.

`D_T` and `T` are structurally equal (`==`) but not strictly equal (`===`).
However, `D_T` is not assignable to `T` because they are different types.
Structural equality does not imply assignability.

Two separate constructions of the same `T` produce the same `D_T` and `D_T` is uniquely determined by the canonical structural identity of `T` within a compilation unit.
The scope of canonicalization is program-wide.

**Example**:

```
// cat=1
// dog.food=steak

function read_structured_string(str string) dynamic {
    let keys []string = empty_slice_helper();
    let values []('terminal string + 'nested dynamic) = empty_slice_helper();
    let buffer []char = empty_slice_char();
    for let i int = 0; i < str.size; i++ {
        let b byte = str.[i];
        match b.to_char() {
            ' ' -> {continue;}
            '=' -> {
                keys.push(buffer.to_string());
            }
            '\n' -> {
                values.push(buffer.to_string());
            }
            '.' -> {
                keys.push(buffer.to_string());
                i++;
                let subkey dynamic = read_structured_string(str.(i..));
                values.push(subkey);
            }
            c -> {
                buffer.push(c);
            }
        }
    }
    return (dynamic)(keys, values);
}
```

When a variable is declared as having type `dynamic`, this acts as a placeholder that is replaced at compile time with the concrete generated type `D_T`.
The placeholder replacement occurs after full type checking of the initializer.
After replacement, the variable no longer has type `dynamic` anywhere in the typed AST.
`dynamic` variables must be assigned a value produced by type synthesis, otherwise it is a compile time error.

```
// Not allowed
let x dynamic = 5;
```

Also, a variable of type dynamic must have an initializer that is a type synthesis expression.

```
// Not allowed
let x dynamic;
```

In function signatures, the return type must be resolved after type checking the body.

- The function's return type is inferred from the `(dynamic)` expressions in all return paths.
- All return paths must produce `(dynamic)` of the same underlying T.
- The underlying `T` must be strictly equal across return paths.
- Structural equality is insufficient.

Therefore the following example is incorrect:

```
if cond {
    return (dynamic)(1);
} else {
    return (dynamic)("x");
}
```

**Example**:

```
let data dynamic = read_structured_data(str);
```

In case of recursion of `dynamics`, the inner dynamic type have already been transformed into a concrete type (`D_U`) so there is no infinite expansion.
The transformation occurs during type checking.
Recursive `dynamic` types are resolved as finite nominal graphs.
`dynamic` does not introduce recursive type capabilities beyond those permitted by the base type system

Because `D_T` is structurally identical to `T` all normal structural rules apply.
This includes field access and pattern matching.
Sum and product semantics are preserved exactly.
`dynamic` introduces no runtime tagging beyond what `T` already required.

```
let keys = data.keys

let values_size = data.values.size
for {let value = data.values.[0]; let i = 0;}; i < values_size; {i += 1; value = data.values.[i];} {
    match value {
        'terminal s -> {...}
        'nested n -> {...}
    }
}
```

> `dynamic` introduces compile-time synthesis of a nominal type equal in structure to the wrapped expression type.

**Cross boundary compatibility**:

```
function f() dynamic { return (dynamic)(1); }
function g() dynamic { return (dynamic)(1); }

let a = f();
let b = g();
```

Here `a` and `b` are the same type.

### Blob type

`blob` represents a contiguous region of memory whose size is known only at runtime.
`blob` is a built-in primitive.
The size of a `blob` value is not part its static type, it is a dynamically-sized value type.

A `blob` value consists of a pointer to memory and a runtime size in bytes.
The memory layout is implementation-defined but it must contain an address and a lenght.

`blob` is always sized at runtime but statically known to occupy a fixed machine representation (pointer + size)

A `blob` has no element type, it is untyped raw storage.

`blob` has no structural knowledge, therefore, it is not possible to access fields, index into it or pattern match it.
Allowd operations include querying its size, copying it, passing it, and performing explicit reinterpretation (raw casting).

A `blob` is an owning conatiner of its memory.
Copying a `blob` performs a deep copy of the underlying memory region.
Move semantics transfer ownership of the `blob` without duplication.
Destruction frees the owned memory.

A `blob` value may be created through one of the following mechanisms:

1. Allocation

   A new `blob` may be created by allocating a contiguous region of memory of a specified runtime size.
   The allocated memory is owned by the resulting `blob`.
   The contents of the memory are unspecified unless otherwise defined.

2. Copy from existing memory

   A new `blob` may be created by copying bytes from an existing contiguous memory region.
   The resulting `blob` owns its memory independently of the source.
   The source memory is not aliased.

3. External source construction

   A `blob` may be created by operations that produce raw byte data (for example, file or network operations).
   The resulting `blob` owns the produced memory.

A `blob` cannot be created via type casting.
A `blob` cannot be implicitly constructed from structured types.

Reinterpretation of a blob as another type T requires explicit raw casting and must obey size compatibility rules, alignment requirements, and have no automatic deserialization.

### Never type

`never` is a built-in primitive type representing computations that do not complete normally.
A value of `never` cannot exist.
A function returninig `never` does not return execution to its caller.

**Example**:

```
funciton panic(message 'Some string + 'None unit) never {
    // ...
}
```

**Semantics**:

- `never` is the bottom type.
- `never` is strictly equal only to itself (`===`).
- `never` is assignable to all types.

No type is assignable to `never`.
This allows:

```
let x int = panic("error");
```

because `panic(...)` has type `never`, and `never` is assignable to `int`.

An expression of type `never` is considered to have any required contextual type.

In control flow constructs:

```
if cond {
    panic("error");
} else {
    5
}
```

The entire expression has type `int`.

The branch containing `panic` does not contribute a type; it is treated as `never`.

In a match expression a branch returning `never` does not affect exhaustiveness.
If all branches return `never`, the match expression has type `never`.

`never` does not participate in `dynamic` type synthesis.
`(dynamic) panic(...)` is invalid because `panic(...)` has no value.

If a function call has type `never`, all statements following it in the same block are unreachable.

## Functions

A _function_ is a named entity that encapsulates executable statements, may accept parameters, and may return a value.

### Function declarations

A function declaration introduces a binding for a function.

**Syntax**:

```
function <identifier>(<parameters>) <return_type> {
    <statements>
}
```

The return type may be omitted if the function returns `unit`.

**Examples**:

```
function add(a int, b int) int {
    return a + b;
}

function greet(name string) {
    print("Hello, " + name);
}
```

### Forward declarations

A function declaration without a body is a forward declaration.
Forward declarations allow mutual recursion and separate interface from implementation.

**Syntax**:

```
function <identifier>(<parameters>) <return_type>[;]
```

A forward-declared function must have a corresponding definition.
Calling a function that has no implementation causes a compile-time error.

### Parameters

Parameters are variables that receive values when a function is called.

**Syntax**:

```
<identifier> <type>, <identifier> <type>, ...
```

Parameter passing is by value: the function receives a copy of each argument.
Modifications to parameters within the function do not affect the caller's variables.

For large values, the implementation may optimize by passing references internally, but this optimization is not observable to the programmer.

### Function application

A function is invoked by applying it to arguments.

> **Note:** When calling a method on a value (e.g., `<value>.<method>()`), the compiler rewrites this as a function call (e.g., `<method>(<value>)`) at compile time.

**Syntax**:

```
<function_identifier>(<arguments>)
<expression>(<arguments>)  // For function values
```

Arguments are expressions separated by commas.
The number and types of arguments must match the function's parameters.

**Examples**:

```
let sum int = add(3, 4);
let result int = compute(x, y, z);
```

### Recursion

Functions may call themselves directly (direct recursion) or through other functions (mutual recursion).
Recursion is permitted by default without special annotation.

**Example**:

```
function factorial(n int) int {
    if n <= 1 {
        return 1;
    }
    return n * factorial(n - 1);
}

// Mutual recursion
function is_even(n int) bool {
    if n == 0 { return true; }
    return is_odd(n - 1);
}

function is_odd(n int) bool {
    if n == 0 { return false; }
    return is_even(n - 1);
}
```

### Anonymous functions

Anonymous functions (lambdas) create function values without binding them to a name.

**Syntax**:

```
function(<parameters>) <return_type> { <statements> }
```

**Examples**:

```
let double (int) -> int = function(x int) int { return x * 2; };

let numbers []int = [1, 2, 3, 4, 5];
let doubled []int = map(numbers, function(x int) int { return x * 2; });
```

### Closures

Anonymous functions capture variables from their enclosing scope, forming closures.
Captured variables are captured by value at the time the closure is created.

**Example**:

```
function make_adder(n int) (int) -> int {
    return function(x int) int { return x + n; };
}

let add_five (int) -> int = make_adder(5);
let result int = add_five(10);  // result = 15
```

### Purity and side effects

A `pure` function:

- Does not modify global state
- Does not perform I/O
- Does not modify its arguments
- Returns the same result for the same arguments

A `const` function additionally:

- Does not read global variables
- Does not read external state (time, random, etc.)

<div class="page"/>

## Memory management

fib provides explicit control over memory allocation.
Variables are allocated on the stack by default.
Heap allocation requires explicit use of the `alloc` statement.

### Stack allocation

Local variables declared within a function are allocated on the stack.
Stack-allocated values are automatically deallocated when the enclosing scope exits.

**Example**:

```
function example() {
    let x int = 42;              // Stack allocated
    let arr [100]int;            // 100 integers on stack
    let point (int * int) = (1, 2);  // Tuple on stack
}  // All variables deallocated when function returns
```

### Heap allocation

The `alloc` statement allocates memory on the heap and binds it to a pointer.

**Syntax**:

```
alloc <identifier> <pointer_type>, <count> = <initalization_expression>;
```

The statement allocates `<count>` elements of the pointed-to type and binds the resulting pointer to `<identifier>`.
The allocated memory is initialized with the value of the `<initalization_expression>`.
The total bytes allocated is `<count> * size_of(<element_type>)`.

**Examples**:

```
// Allocate 10 integers with unique ownership
alloc numbers unique &int, 4 = [1, 2, 3, 4];
deref numbers = 42;              // Set first element
// [42, 2, 3, 5]

// Allocate with shared ownership
alloc buffer shared &byte, 1024 = ZeroArray:new(1024);

@unsafe {
    alloc raw_data &int, 100 = ZeroArray:new(100);
    // Must be manually freed
}
```

### Deallocation

Safe pointers (`unique &T`, `shared &T`) are automatically deallocated:

- `unique &T`: Freed when the pointer goes out of scope or is moved.
- `shared &T`: Freed when the reference count reaches zero.

Raw pointers must be manually deallocated using the `free` statement.

**Syntax**:

```
free <pointer_expression>
```

**Example**:

```
@unsafe {
    alloc p &int, 10 = [1,1,2,2,3,3,1,1,2,2];
    // ... use p ...
    free p;              // Manual deallocation required
}
```

Freeing a null pointer is a no-op.
Freeing an already-freed pointer causes undefined behavior.

### Deferred deallocation

The `defer` statement schedules a statement to execute when the current scope exits.
This is useful for ensuring resources are released.

**Syntax**:

```
defer <statement>
```

**Example**:

```
@unsafe {
    alloc buffer &byte, 4096 = ZeroArray:new(4096);
    defer free buffer;           // Will execute when scope exits

    // ... use buffer ...

    if error_condition {
        return;                  // buffer is freed here
    }

    // ... more code ...
}  // buffer is freed here
```

Deferred statements execute in reverse order of their lexical declaration (LIFO).

Deferred statements execute in the following scenarios:

- Normal scope exit
- Early returns
- Panic unwinding
- Coroutine stack unwinding

### Arena allocation

Arenas provide a pattern for bulk allocation and deallocation.
Arenas are implemented in the standard library using the primitive allocation facilities described above.

See the standard library documentation for arena usage patterns.

<div class="page"/>

## Compiler Hints

_Compiler hints_ are annotations that provide additional information to the compiler for optimization, verification, code generation, or semantic enforcement.

Hints do not change the semantics of correct programs but may affect performance, diagnostics, and runtime behavior.

Some hints may relax safety checks and should be used with care.
Some hints may enforce stricter rules.

_Hinting_ is the process of adding a hint to an entity.

### Hint Syntax

Hints are prefixed with the `@` symbol and may appear before declarations, statements, or expressions depending on the hint type.

**Syntax**:

```
@<hint_name>
@<hint_name>(<arguments>)
```

Multiple hints may be applied to the same element:

```
@hint1 @hint2 @hint3
function example() { ... }
```

Hints may also be written on separate lines:

```
@hint1
@hint2
function example() { ... }
```

### Parameter Hint Semantics

Hints applied to function parameters have two distinct modes based on their syntactic placement: **requirement hints** and **promise hints**.
This syntactic distinction determines whether the hint describes what the caller must provide or what the function guarantees.

#### Requirement Hints (Inline Placement)

When a hint is placed **inline** with the parameter declaration (before the parameter name), it is a _requirement hint_.
The caller must guarantee that the passed argument satisfies the specified property.
The compiler enforces this requirement at call sites.

**Syntax**:

```
function <identifier>(@<hint> <parameter> <type>, ...) <return_type> { ... }
```

**Example**:

```
function simd_process(@aligned(32) data []float, @nonnull config Config) {
    // Caller MUST provide:
    // - 32-byte aligned data
    // - Non-null config
    // Function can assume these properties hold
    ...
}

@aligned(32)
let vectors []float = [...];
simd_process(vectors, config);  // Valid: vectors is aligned

let unaligned []float = [...];
simd_process(unaligned, config);  // Compile-time error: alignment not proven
```

Requirement hints enable interprocedural optimizations and compile-time safety checks.
The function may assume the required properties hold without runtime verification.

> In the example above, `@align(32)` does not align the values. The hint simply provides the compiler the information that the data is aligned.

#### Promise Hints (External Placement)

When a hint is placed **outside** the function signature with a parameter binding (using `@<hint>(<parameter_name>)` syntax), it is a _promise hint_.
The function promises to uphold the specified behavior regarding that parameter.
The caller is not constrained; any compatible value may be passed.

**Syntax**:

```
@<hint>(<parameter_name>)
function <identifier>(<parameter_name> <type>, ...) <return_type> { ... }
```

**Example**:

```
@readonly(src)
@writeonly(dest)
function copy_buffer(src []byte, dest []byte) {
    // The function promises:
    // - It will not modify src
    // - It will only write to dest, not read from it
    for let i = 0; i < len(src); i++ {
        dest.i = src.i;
    }
}

// Caller can pass any []byte, mutable or immutable
let data []byte = [1, 2, 3];
let buffer []byte = [0, 0, 0];
copy_buffer(data, buffer);  // Valid: no constraints on caller
```

Promise hints enable local optimizations within the function.
The compiler verifies that the function upholds its promises and issues a compile-time error if violated.

#### Combining Requirement and Promise Hints

A parameter may have both requirement and promise hints:

```
@readonly(config)
function process(@immutable config Config, @nonnull output []byte) {
    // config: caller guarantees immutability, function promises not to modify
    // output: caller must provide non-null (no promise about modification)
    ...
}
```

In this example:

- `config` has an inline `@immutable` requirement (caller must provide immutable data) and an external `@readonly` promise (function won't modify)
- `output` has only an inline `@nonnull` requirement (caller must provide non-null, function unconstrained in how it uses it)

#### Verification Rules

Promise hints are verified to be upheld by the constrained function. If the compiler can statically determine that the function does not uphold the promise it will throw a comile time error.

**Example**:

```
@writeonly(buffer)
function read_buffer(buffer []byte) {
    let new_buf []int = buffer;
}
```

Here the compiler will throw an error at compile time because it can determine that buffer is being read while being hinted that it is write-only.

```
@writeonlt(buffer)
function read_buffer(buffer []byte) {
    let new_buf []int = use_buffer(buffer);     // use_buffer does not read from buffer, OK
}
```

When variables are passed to other functions or other forms of control flow, the compiler will attempt statically validate the promise in all possible flows.

Requirement hints are only validated at the moment of function calling.

```
function foo(@readonly x int) {
    // ...
}

// not allowed
let bar int = 10;
foo(bar);

// allowed
@readonly
let bar int = 10;
foo(bar);
```

### Hint Placement Summary

| Hint Category             | Placement                                      | Semantics                      |
| ------------------------- | ---------------------------------------------- | ------------------------------ |
| Function hints            | Before `function` keyword                      | Applies to function            |
| Variable hints            | Before `let` keyword                           | Applies to variable            |
| Parameter hints (require) | Inline, before parameter name: `@hint param`   | Caller must guarantee property |
| Parameter hints (promise) | External, before function: `@hint(param_name)` | Function promises behavior     |
| Loop hints                | Before `for` keyword                           | Applies to loop                |
| Branch hints              | Before `if` keyword                            | Applies to branch              |
| Expression hints          | Before the expression                          | Applies to expression          |
| Statement hints           | Before the statement                           | Applies to statement           |
| Type hints                | Before `type` keyword                          | Applies to type                |
| Module hints              | Before `module` keyword                        | Applies to module              |

## Stackful coroutines

Stackful coroutines are baked into the language.

A stackful coroutine may be declared using the `coroutine` keyword. Semantically, this will define a function that can be suspended and resumed later.

> Technical note: because these are stackful coroutines, when they are suspended, their stack frames, including local variables and spilled arguments, along with registers and other relevant memory, are stored in a heap allocated object.

**Syntax**:

```
coroutine <coroutine-name>(<arguments>) <return-type> {<body>}
```

Inside the body of a coroutine, the `yield` statement may be used to suspend the execution of the coroutine.

**Example**:

```
coroutine counter(start int, end int) 'Some int + 'None unit {
    let i int = start;
    for i < end {
        yield 'Some i;
        i += 1
    }
    yield 'None ();
}
```

> The return type expressed int the coroutine's signature is the type of the value produced by the `resume` expression and not the handler type produced by the `spawn` expression.

A coroutine may be spawned using the `spawn` expression. Spawning a coruoutine does not begin the execution of its body statements, instead, it produces a _coroutine handler_.

The `resume` expression utilizes a coroutine handler and execute its coroutine

```
let c = spawn counter(0, 3);

for {
    let result = resume c;
    match result {
        'Some x  -> {print(x);}
        'None () -> {break;}
    }
}
// 0, 1, 2
```

The `spawn` expression returns a _coroutine handle_, which is similar to a function type.

```
let c coroutine -> 'Some int + 'None unit = spawn counter(0, 3);
```

The type of `c` is a coroutine handler that produces an int and it is written as `coroutine -> 'Some int + 'None unit`.

The coroutine handler type is an opaque first-class citizen of the language.
The handler is not callable like a normal function, `resume` must be used.

Handlers use move semantics. Trying to bind a handler to another identifier is not allowed.

```
let c = spawn counter(0, 3);
// Not allowed
let d = c;
```

Handlers may be referenced using pointers.
Aliasing is allowed, so shared references of the handler is also allowed.
If no aliasing is desired use unique pointers.

```
let c = spawn counter(0, 3);

let p1 shared &_ = copy(c);
let p2 shared &_ = addressof c;

// use them
let x1 = resume (deref p1)          // 0
let x2 = resume (deref p2)          // 1
```

Concurrency should be handled using other language structures like mutex or semaphores.

Dropping an uncompleted coroutine performs stack unwinding and cleanup must is perfomed by `defer` blocks.
Unwinding walks frames from innermost to outermost, executing each frame’s defers before discarding it.

## Code generation

Code generation code should be placed in a "\*.codegen.fib" file.

```
codegen "<name-of-codegen>" (<regex>, <substitution-strings>) { <template> }
```

```
codegen "Linked List" ("LinkedList_()", "_T") {
    type LinkedList__T = (
        'head unique &_T *
        'tail unique &_T
    )
}
```

```
codegen "foo example" ("foo_()_()", "_T", "_U") {
    function foo__T__U() unit {...}
}
```

The name is used to identify the codegen execution during debugging.
The regex is used to find instances in the code base where the expression matches.
Capture groups may be used.
Capture groups are assigned to each substition string in order.
In the "foo example" the first capture group is assigned the "\_T", the second is assigned the "\_U" string and the list would go on.
If the amount of capture groups and substituion strings is not matched this codegen fails and an error is thrown.
If there is no matching code in the code base the codegen is skipped (codegen can be used to conditionally add code based on the existance of code or not).
The substitution string is replaced in the template with the value that was captured in the regex.
The template is valid fib code.

Matching is perfomed against identifiers.

In case of collision errors there is a compile time error.

Generated code is injected into the module the match occurred.

Codegen runs before contract monomorphization.

Order: Parse → Codegen → Name resolution → Type check → Contract monomorphization → Lowering

Only one pass is allowed and codegen cannot be written inside template.

User can configure the amount of instantiations that a given codegen can perform
