# Planned name resolution and compile-time bindings

> **Design decisions, not current compiler behavior.** Fib currently documents `import`
> in [Imports and Modules](imports.md); the `comp`/`pub` namespace model and
> named control-flow targets below are not yet implemented. Examples illustrate
> intended semantics, not programs that necessarily parse today.

## Files, paths, and visibility

A source file is a namespace. `::` statically selects a name; intermediate
segments of a path must denote files or compile-time aliases to files. A
function or variable may be the final segment, but cannot be traversed with
another `::segment`. Within a file, its declarations are accessible without a
file qualifier. A file-qualified name can still select a file declaration when
an unqualified use resolves to a function-local binding of the same spelling.

Functions, top-level variables, and top-level `comp` bindings (including types)
are private to their file by default. Prefix a declaration with `pub` to make
that binding accessible from other files. An alias can be exposed with `pub`
even when it refers through a private alias in the exporting file; however, the
exporting file must itself have access to the target. Two aliases may denote the
same underlying declaration. An alias cycle with no underlying entity is an
error.

The existing `import` mechanism is intended to give way to qualified paths and
aliases. The rule that maps an initial file-name segment to a source file is
**not yet decided**; it is separate from selecting members of an already-known
file namespace.

## `comp` versus `const`

`comp` binds a compile-time entity, not necessarily a compile-time-evaluated
number. It can alias a static path to a file, function, top-level variable, or
type, and can also bind a newly constructed type. It does not promise arbitrary
compile-time arithmetic evaluation. Its aliases have no separate runtime
storage; an alias to a mutable top-level variable denotes that variable's same
storage and permits assignment to it. `comp` aliases may be declared within
functions, but cannot alias parameters or function-local variables.

```fib
comp other_file := path::to::other;
comp f := other_file::function;
comp counter := other_file::counter; // assigning to counter writes the original
pub comp Color: type = struct { r: uint4, g: uint4, b: uint4 };
```

`const` instead declares a new immutable variable. Its initializer may be an
ordinary expression, such as `2 + 3`; reading a mutable variable initializes a
new value rather than creating a live view of its storage. An immutable pointer
binding alone does not make the pointed-to storage immutable.

```fib
const snapshot := other_file::counter; // does not change if counter changes
const three_more := 2 + 3;
```

Types and functions are intended to be first-class. Named types use `comp`
(e.g. `pub comp Color: type = struct { ... };`), without an implicit `comp` from
`: type`. A `comp` alias to a function denotes its declaration; a `const`
initialized from a function holds a callable function value. Function values
are planned as plain function pointers, without captured local state. APIs
that require per-callback state pass a separate context pointer; callers
manage its lifetime. Whether to allow nested functions without captures is
still undecided.

## Lexical names and labels

Names declared within a function cannot redeclare an enclosing parameter or
local name. Sibling blocks can reuse a name, as can different functions. A
function parameter may share a spelling with a file-level declaration: inside
the function the unqualified spelling denotes the parameter, while the
file-qualified path denotes the file declaration. Struct fields do not conflict
with these lexical names.

Named control-flow targets are a separate namespace from variables, functions,
and files. Their references begin with `::`, unlike static selection, which
begins with a file namespace or alias. Thus a local named `outer` does not
conflict with a control-flow label `outer`. The exact label declaration syntax
and which targets `break` and `continue` may reach remain to be specified.

## Forward references and initialization

Bindings may resolve names declared later in the same file. Top-level variable
initializers nevertheless run in declaration order: reading a later variable
before it has been initialized is an error, with a diagnostic pointing to the
later initialization. This initialization rule is distinct from name lookup.
Alias-only cycles are rejected; the rules for recursive types and other
value-dependency cycles still need to be specified.
