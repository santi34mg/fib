# Pointers and memory

> **Verified parser behavior:** Raw pointer types, null literals, restricted postfix address/deref expressions, and casts parse.

## Pointer types and null

```fib
pointer: *int32;
buffer: *void;
next: *Node = null;
if next == null { found := false; };
```

`*T` is a recursively parsed pointer type. `*void` is syntactically valid;
`null` is a literal node.

No `unique`, `shared`, or `weak` pointer grammar exists. Leftover helper names or
historical ownership plans should not be described as usable pointer kinds.

## Address-of and dereference

```fib
value: int32 = 42;
address := value&;
loaded := address*;
field := (node*).next;
converted := (address&) as *void;
```

The parsed postfix operators are `expr&` (address-of) and `expr*` (dereference),
without a dot. They are consumed only before `;`, `)`, `]`, `}`, or `,`.
For subsequent member access, casts, or arithmetic, group the postfix operation:
`(node*).next` rather than `node*.next`. Prefix `*pointer` and `&value` do not
parse. Old `expr.*` and `expr.&` forms are rejected.

## Index and field targets

```fib
item := pointer[i];
pointer[i] = item;
(node*).next;
```

Indexing has no dot before `[`. The parser retains an index expression/assignment.
Ordinary name-starting field assignments also parse (`obj.field = value;`).

Direct pointer stores are not a completed syntax feature: `pointer* = value;`
does not trigger postfix dereference before `=`, prefix `*pointer = value;` is
rejected, and a parenthesized target statement `(pointer*) = value;` is not
handled as assignment. Do not translate old `pointer.* = value` examples into
claimed working stores. Dedicated AST assignment variants are not evidence that
the parser emits them.

## Casts and external allocation declarations

```fib
extern fn malloc(size: usize) *void;
extern fn free(ptr: *void) void;
buffer := malloc(16);
node := buffer as *Node;
address := node as usize;
defer free(buffer);
```

These are accepted declaration/call/cast/defer shapes, not memory-management
recipes. The old `import std::libc` declaration is not parsed.

See [Casting](casting.md), [Control flow](control-flow.md), and
[Imports](imports.md). Pointer-design proposals belong in the [roadmap](../roadmap/).

Related follow-up is collected in this [GitHub issue #11](https://github.com/santi34mg/fib/issues/11).

Parser coverage: `test_address_of`, `test_dereference`,
`test_postfix_access_keeps_binary_operators`, `test_old_postfix_access_forms_are_not_accepted`,
and `unsupported_deref_and_multi_assignment_syntax_errors`.
