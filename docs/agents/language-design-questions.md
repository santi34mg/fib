# Language Design Questions

These questions track the decisions requested by the
[improvement roadmap](improvement-roadmap.md). Write answers directly beneath
each section. Unanswered questions remain open; implementation that depends on
them is pending those answers.

## 1. Numeric operations and conversions

- What implicit numeric conversions and promotions are permitted in operators,
  assignments, function arguments, and return values?
- What are the rules for explicit narrowing and signed/unsigned conversions?
- How is an integer literal's type determined, and when is an out-of-range
  literal rejected?
- What happens on integer overflow, including negation, division, and shifts?
- What happens on division or remainder by zero, and do compile-time and runtime
  behavior differ?
- What are the rules for conversions between integers and floating-point values,
  including out-of-range values and NaNs?

**Answers:**

- The implicit numeric conversions allowed are conversions from a narrow-width 
number (signed/unsigned integer or floating-point) into a wider number. For 
instance, a `@uint2` can be converted into a `@uint4` but a `@float2` cannot be
converted into a `@float`. Implicit conversion from signed to unsigned or the
other way around is not allowed. Implicit conversion from integer types to 
floating-point types is not allowed. 
- Explicit narrowing truncates the value from the left. That means that 
`@uint2 as @unit` transforms `0xABCD` into `0xCD`. Sign conversion performs a 
reinterpretation of the value. That means that the `@uint` value `255` 
(`0b11111111`) becomes `-1` when casted as an `@int`. Integer-float conversions 
(`@uint as @float` or `@float as @uint`) does not perform a bit reinterpretation
and instead does the proper parsing and coercion.
- On integer overflow all operations should wrap. The following `@uint8` 
expression `MAX_UINT8 + 1` would result in `0` while the `@int8` expression
`MAX_INT8 + 1` would equal `MIN_INT8`. Negation, division and shifts work the 
same way, always wrapping. For other behaviours such as trapping, builtin 
functions will be provided.
- Division or remainder by zero *traps*. If division by zero is determined in 
compile time, that is a compile time error. 
- For cases such as `@float4 as @int4`, when the floating point number is NaN,
plus or minus infinity or a finite value that is out of range, then de default
behaviour when trying to cast is to *trap*. In the case of `@int4 as @float4`
even if the conversion is not precise the compiler will not trap.
- For operations that result in a *trap* (division by zero, conversion of float
to int), builtin `@try_*(...)` functions will be provided.

## 2. `null` and `@never`

- Which types may receive or be compared with `null`?
- What type does `null` have before a contextual type is available?
- Where may `@never` appear, and how does it affect type checking and control-flow
  analysis?
- What obligations does a function declared to return `@never` have?

**Answers:**

- Only pointer types (`T*`) can be compared against `null` or be assigned a 
`null` value.
- `null` should default to `*@void`.
- `@never` is a type that may only appear in a function signature as the return
type of the function and it indicates that the function never returns. It differs
from `@void` which indicates that no return value is provided. `@never` is used
for functions like `panic`, `exit` or `error` which might end the program 
immediately.
- A function declared to return `@never` should have no `return` statement and 
otherwise has no obligation.

## 3. Separators and semicolons

- Where are commas required, optional, or forbidden, including trailing commas?
- Where are semicolons required, optional, or forbidden?
- Do newlines affect statement or expression boundaries?
- What statements are permitted in each loop-header position?

**Answers:**

- Commas are required between all elements of lists like in the cases of 
elements of array initialization, member declarations in structs and enums and 
between function parameters. Trailing commas are optional.
- Semicolons are required at the end of all statements.
- Newlines do not affect statement or expression boundaries.
- All statements are allowed in the loop-header.

## 4. `defer` and evaluation order

- Which scope owns a deferred statement, and when does that statement run?
- In what order do multiple deferred statements run?
- How does deferred work interact with normal fallthrough, return, break,
  continue, and nested loops or blocks?
- When is a return expression evaluated relative to deferred work?
- When are the arguments and values used by deferred work evaluated or captured?
- Which statements and control-flow operations are permitted inside `defer`?

**Answers:**

- The inner most scope is the one that owns the deferred statement and the 
deferred statment runs when that scope ends.
- Deferred statments run in inverse order of declaration.
- Whenever the scope ends (because of normal fallthrough, returns, break or 
continue) the deferred statments are executed. In the case of nested loops,
the deferred statment is executed when the end of the inner most scope that
contains the `defer` ends.

```fib
    // ... 
    for (path := std::fs::dir::ls(dir_path)) {
        file := std::core::io::read(path);
        defer std::core::io::close(file);

        contents := std::string::readFile(file) else (err) {
            std::core::io::printf("Error: %s\n", err);
            continue; // closes the file
        };

        std::core::io::printf("Path: %s\n", path);
        for (line := std::string::splitNewLine(contents)) {
            std::core::io::printf("%s\n", line);
        };
        // closes the file
    };
    // ...
```
- The deferred work is evaluated before actually returning to the caller.
However, let's say this code happens:
```fib
fn example() @void {
    statement1;
    defer statement2;
    statement3;
    return expression1;
}
```
The final order would be:
1. statement1
2. statement3
3. evaluate expression1
4. statement2
5. return value of expression1
- The arguments and values used by deferred work are captured the moment the 
deferred work is about to begin.
- `defer` inside `defer` is not allowed. `return` inside `defer` is not allowed.
`if`, `while`, and `switch` are allowed inside `defer`


## 5. Strings and allocation

- What are the ownership, lifetime, and mutability rules for `@string`?
- How are strings represented, and how is their length defined?
- What is the behavior of embedded NUL characters in Fib operations and at FFI
  boundaries?
- Who owns strings passed to or returned from foreign functions?
- How must allocation failure be reported and handled by standard-library APIs?

**Answers:**

- 

## 6. Modules and declaration visibility

- Which declarations are visible outside their defining module?
- What are the rules for re-exporting imported declarations?
- How are ambiguous imports and collisions between imports and local
  declarations handled?
- Which duplicate declarations, aliases, parameters, fields, variants, and local
  bindings are permitted?
- What are the shadowing rules across nested scopes?
- How should conflicting declarations of the same foreign symbol be handled?

**Answers:**

<!-- Write your answers here. -->

## 7. Slice lifetimes and escapes

- Who owns the storage referenced by a slice, and how long must it remain valid?
- Under what conditions may a slice of local or temporary storage be returned,
  stored, or passed to another function?
- What lifetime obligations apply to slices received from callers or foreign
  functions?
- What are the rules for mutable slices, aliasing, and reassignment of the
  underlying storage?
- What should happen when the compiler cannot establish that a slice remains
  valid?

**Answers:**

<!-- Write your answers here. -->

## 8. Floating-point comparisons

- What results should equality, inequality, and ordered comparisons produce
  when either operand is NaN?
- What rules apply to signed zero and infinities?
- What floating-point behavior must optimization preserve?

**Answers:**

<!-- Write your answers here. -->

## 9. Reserved syntax and C-compatible unions

- What syntax and semantics should an untagged `union` declaration have?
- How is a union initialized, and which field reads and writes are permitted?
- How is field-access safety expressed or checked?
- What C layout and ABI guarantees must unions provide, including when nested
  inside aggregates or passed by value?
- What restrictions apply to unions and other aggregates at FFI boundaries?
- What syntax and semantics should function values have, and what remains
  reserved until they are implemented?
- What are the initialization, evaluation, mutability, and visibility rules for
  module constants?

**Answers:**

<!-- Write your answers here. -->

## 10. Release mode and optimization

- What behavior should `--release` select?
- How should `--release` interact with an explicitly supplied `-O` level?
- What should the public option names and defaults be for optimization and
  runtime bounds checking?

**Answers:**

<!-- Write your answers here. -->
