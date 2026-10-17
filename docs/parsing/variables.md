# Variables

> **Verified parser behavior:** Declarations and assignments below parse into syntax ASTs.

## Explicit and inferred declarations

```fib
count: int32 = 0;
head: *Node;
total := 42;
const limit: int32 = 10;
const snapshot := total;
```

`name: Type` may omit its initializer. `name := expression` records a declaration
with no explicit type. `const` records an `is_const` flag for that binding.
Even `const count: int32;` parses.

The old `var Type name = expression` declaration form is not supported. `var`
is currently an ordinary identifier, not a declaration keyword.

## Multiple bindings

Each binding has its **own** optional `const` and type annotation. A qualifier
or type does not propagate to later bindings.

```fib
q, r := divmod(17, 5);
const a: int8, b: string = 1, "text";
a: int8, const b := 1, 2;
left: int32, right: int32;
```

In the second example only `a` is const; in the third only `b` is const.
Bindings without an initializer must each have an explicit type:
`a, b: int32;` is rejected. With initializers, earlier bindings may omit types,
but the final binding must have `: Type` or `:=`. The final `:=` introduces the
shared initializer list, not a type shared by the bindings.

The AST preserves bindings and initializer expressions separately.

## Assignments

```fib
count = 2;
obj.inner.value = 42;
items[i] = 5;
count += 1;
count -= 1;
count *= 2;
count /= 2;
count %= 3;
```

Plain assignment starting from a name can retain a field/index expression as
its target. Compound assignment requires a **single unqualified variable name**;
`items[i] += 1;`, `obj.field += 1;`, and `file::count += 1;` are rejected. The
parser desugars compound assignment to an assignment with a binary RHS.

The compound-target restriction is a parser limitation, not a settled language
design rule.

Multiple **declaration** syntax is supported, but multiple **assignment** syntax
`q, r = divmod(17, 5);` is not, despite a `MultiAssignment` AST variant.
Pointer-store syntax is also incomplete; see [Pointers and memory](pointers-memory.md).

## Terminators

Statements require `;`; newline does not terminate a statement. This includes
`if`, `while`, and `switch` statements after their closing braces. Top-level
function definitions and `type` declarations are exceptions: they are parsed
by separate declaration routines and do not take an extra trailing `;`.
Bodyless function signatures use `;`. See [Functions](functions.md).

Related follow-up is collected in this [GitHub issue #10](https://github.com/santi34mg/fib/issues/10).

Parser coverage includes `test_const_inferred_declaration`,
`test_multi_var_declaration_inferred_type`, `test_typed_multi_variable_declarations`,
`test_mixed_multi_variable_bindings`, `index_assignment_preserves_target_and_value`,
and `unsupported_deref_and_multi_assignment_syntax_errors`.
