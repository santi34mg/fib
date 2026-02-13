# Lowering to SSA

Here I will discuss how lowering an AST into an SSA-like IR should function for
fib.
This is not a formal document, this is for internal development documentation.

## Expressions

While lowering, when the compiler encounters an expression node, it should match
its variant to decide the instructions to emit.

Expressions can be of the following types:

- Literal (contain only a literal)
- Binary (contain a left and right expression, as well as an operator)
- Unary (contain an expression and an operator)
- Identifier (contains the name of the identifier as a string)
- Grouping (contains another expression)
- Call (contains a callee and arguments, which are an expression and a vector of
  expressions respectively)

### Literal expressions

If the compiler encouters a literal expression, it should emit an instruction
that denotes that value (remember SSA treats variables as values).

```
1
```

Has the following AST:

```
Expression(
    Literal(
        Integer(
            1,
        ),
    ),
)
```

This should get converted into:

```
x_0 = 1
```

### Binary expressions

While lowering, when the compiler encounters a binary expression it should
recursively evaluate its left and then right (for left associativity) branches.
Then it should join them using the operator (use pattern matching).

```
1 + 2
```

Should get converted into:

```
x_0 = 1
y_0 = 2
z_0 = x_0 + y_0
```

A more complex example might be:

```
1 + 2 + 3
```

```
Expression(
    Binary {
        left: Binary {
            left: Literal(
                Integer(
                    1,
                ),
            ),
            operator: Plus,
            right: Literal(
                Integer(
                    2,
                ),
            ),
        },
        operator: Plus,
        right: Literal(
            Integer(
                3,
            ),
        ),
    },
),
```

This should get converted to:

```
x_0 = 1
y_0 = 2
z_0 = x_0 + y_0
a_0 = 3
b_0 = z_0 + a_0
```

### Unary expressions

Recursively evaluate the expression and then emit instruction with operator.

Source:

```
!true
```

AST:

```
Expression(
    Unary {
        operator: Not,
        expression: Literal(
            Boolean(
                true,
            ),
        ),
    },
),
```

Gets converted to:

```
x_0 = true
x_1 = NOT x_0
```

### Identifier

> "In most SSA-based IRs, using an identifier simply means referencing the current SSA value bound to that name. No instruction needs to be emitted." - ChatGPT

### Grouping

Grouping contains another expression. That expression must be evaluated.
No new operation should be performed.

```
(1+2)
```

```
Expression(
    Grouping(
        Binary {
            left: Literal(
                Integer(
                    1,
                ),
            ),
            operator: Plus,
            right: Literal(
                Integer(
                    2,
                ),
            ),
        },
    ),
),
```

```
t_0 = 1
t_1 = 2
t_2 = t_0 + t_1
```

### Call

A function call is interesting.
The callee is an expression itself. So the callee must be lowered first.
Then each argument needs to be lowered.
Finally, a call to the function must be made.

Compiler already resolved that `f` is a function because it performed a type-check.

```
f()
```

Gets converted to:

```
t_0 = call f()
```

If an argument is passed:

```
f(31)
```

Gets converted to:

```
// lower argument
t_0 = 31
t_1 = call f(t_0)
```

If callee is more complicated:

```
get_handler()(40)
```

Lower the callee first, then lower arguments, then call.

```
// lower the callee
t_0 = call get_handler()

// lower arguments
t_1 = 41

// call
t_2 = call t_0(t_1)
```

> Why doesn't `f` need to be "loaded"? Lowering an expression produces a value; emitting instructions is an implementation detail.

The call instruction is value-producing even for unit-returning functions.

## Statements

Statements can be of the following types:

- Variable Declaration (has an identifier, a type and an expression)
- Assignment (has an identifier and an expression)
- Expression
- Function Declaration (has a signature and a body)
- Return (optionally has an expression)
- If (has a clause, a then branch and an else branch)

### Variable Declaration

First of all, variable initalization is just a declaration and an assignment.
The expression must be evaluated first.

```
let x int = 5;
```

Gets converted to:

```
t_0 = 5
x_0 = t_0
```

SSA lowering assumes the declaration's and the expression's types are matching
because a previous type=checking pass was performed.

In the case of uninitalized declarations, that results in a no-op.
Reading from an unitialized variable should error during name resolution.

### Assignment

Lower the expression first and assign its last value.

```
let x int = 5;
x = 10;
```

Gets converted to:

```
t_0 = 5
x_0 = t_0
t_1 = 10
x_1 = t_1
```

> "Assignments do not mutate an SSA value; they create a new SSA name associated with the same source-level variable." - ChatGPT

> SSA lowering assumes name resolution has already assigned each identifier use to a specific declaration

### Expression

Lowering expressions was defined in [expressions](#expressions)

### Function Declaration

For this we need to better define basic blocks. They have a label, zero or more SSA instructions and exactly one terminator.

A function declaration defines the entry point to a basic block. The contents of the basic block will be determined by the contents of the function body.

```
function f(arg1 int, arg2 int) unit {
    arg1 + arg2;
}
```

Gets converted to:

```
f:
    t_0 = arg1_0 + arg2_0
```

### Return

A return is a terminator and after it no more instructions in the block should execute.
Return does two things, consumes a value and ends current basic block.

```
return x + 1;
```

Gets converted to:

```
t_0 = x_0 + 1
return t_0
```

Even if no value is returned an instruction for termination should be emitted.

```
return;
```

Gets converted to:

```
return
```

### If statements

```
if x > 1 {
    y = 1
} else {
    y = 2
}
```

Gets converted to:

```
entry:
    t_0 = x_0 > 1
    branch_if t_0 true_br false_br

true_br:
    y_0 = 1
    branch merge

false_br:
    y_1 = 2
    branch merge

merge:
    y_2 = phi(y_0, y_1)
```

### For

Infinite loop:

```
let x int = 0;
for {
    x = x + 1
}
```

Gets converted to:

```
entry:
    x_0 = 0
    branch loop

loop:
    x_1 = phi(x_0, x_2)
    x_2 = x_1 + 1
    branch loop
```

While-like for loop:

```
let i int = 0
for i < 10 {
    i = i + 1
}
```

Gets converted to:

```
entry:
    i_0 = 0
    branch loop_header

loop_header:
    i_1 = phi(i_0, i_2)
    t_0 = i_1 < 10
    branch_if t_0 loop_body exit

loop_body:
    i_2 = i_1 + 1

exit:
    ...
```

C-style for loop:

```
for let i int = 0; i < 10; i++ {
    let a int = 12;
}
```

Gets converted to:

```
entry:
    i_0 = 0
    branch loop_cond

loop_cond:
    i_1 = phi(i_0, i_2)
    t_0 = i_1 < 10
    branch_if t_0 loop_body exit

loop_body:
    a_0 = 12
    branch loop_step

loop_step:
    i_2 = i_1 + 1
    branch loop_cond

exit:
    ...
```

> Variables declared inside the loop body (a_0) are scoped to that block and do not require φ nodes unless they are used outside the loop
