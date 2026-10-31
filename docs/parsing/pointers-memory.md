# Pointers and memory

The confirmed [pointer rules](../struct-storage-semantics.md) add shallow
writable-to-readonly pointer/slice views and `pointer.field` shorthand with exactly
one implicit dereference and a warning. Both are implemented; checked slice execution also works. The shorthand
preserves readonly permissions and ordinary null checks.

> **Verified parser behavior:** Raw pointer types, null literals, restricted postfix address/deref expressions, and casts parse.

## Pointer types and null

```fib
pointer: *int32;
buffer: *void;
next: *Node = null;
if next == null { found := false; };
```

`*T` is a recursively parsed pointer type. `const *T` qualifies its pointee
independently of binding `const`; `const p: *T` and `p: const *T` have different
permissions. The same qualifier syntax applies to slice/array elements.
`*void` is syntactically valid; `null` is a literal node.

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
without a dot. They are consumed before supported delimiters, including `=`
for projection assignments.
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

Direct postfix stores such as `pointer* = value;`, `(pointer*) = value;` and
`(pointer*).field = value;` parse as projection assignments. Prefix
`*pointer = value;` and legacy `pointer.*` remain unsupported. Semantic analysis
checks element permissions; parsing does not imply storage lowering support.

## Casts and external allocation declarations

```fib
callconv "C" fn malloc(size: usize) *void;
callconv "C" fn free(ptr: *void) void;
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
