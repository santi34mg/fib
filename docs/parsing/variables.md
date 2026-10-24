# Variables

Unified declarations are confirmed. The reviewed
[visibility design](../language-specification.md#names-packages-and-source-discovery)
supports bare `pub`, `pub(dir)` and `pub(root)` on declarations.

> **Verified parser behavior:** Declarations and assignments below parse into syntax ASTs.

## Explicit and inferred declarations

```fib
count: int32 = 0;
head: *Node;
total := 42;
const limit: int32 = 10;
const snapshot := total;
comp Alias: type = int32;
pub const exported := 42;
```

Unified declarations use `[pub] [const | comp] name (: Type = | :=) expression;`.
`pub` is optional and precedes the optional, mutually exclusive `const` or `comp`.
The AST records independent `is_pub`, `is_const`, and `is_comp` flags; parsing
these modifiers does not implement visibility or compile-time evaluation.
`name := expression` records a declaration with no explicit type. For
compatibility, typed declarations may still omit their initializer, including
`head: *Node;` and `const count: int32;`.

Type definitions and aliases use the same binding syntax, for example
`Node: type = struct { next: *Node };`. Explicit type syntax produces a
`TypeValue` initializer, not a separate type-declaration statement. Initializers
are arbitrary expressions: named references and type-producing calls remain
ordinary expressions, with type/value discrimination left to semantic analysis.
Legacy `type Name ...` declarations are no longer accepted. See [Types](types.md).

The old `var Type name = expression` declaration form is not supported. `var`
is currently an ordinary identifier, not a declaration keyword.

## Multiple bindings

Each binding has its **own** optional `[pub] [const | comp]` modifiers and type
annotation. Neither modifiers nor types propagate to later bindings.

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
function definitions are the exception: they do not take an extra trailing `;`.
Type-valued bindings require a trailing `;`, including after a struct or enum's
closing brace. Bodyless function signatures use `;`. See [Functions](functions.md).

Related follow-up is collected in this [GitHub issue #10](https://github.com/santi34mg/fib/issues/10).

Parser coverage includes `test_const_inferred_declaration`,
`test_multi_var_declaration_inferred_type`, `test_typed_multi_variable_declarations`,
`test_mixed_multi_variable_bindings`, `index_assignment_preserves_target_and_value`,
and `unsupported_deref_and_multi_assignment_syntax_errors`.
