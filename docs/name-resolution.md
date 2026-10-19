# Name resolution: implementation status and design

> **Boundary:** Tokenization/parsing and lexical table operations exist. The
> analyzer partially validates named units, collects declarations, and traverses
> name uses. Name resolution is incomplete; path/alias and type passes,
> initialization validation, and lowering remain unfinished. The phased tests
> specify intended behavior and deliberately fail where it is not implemented.

## What the parser records today

```fib
Count: type = int32;
const snapshot := other::counter;
fn read(value: Count) Count {
    result := other::calculate(value);
    return result;
}
```

Names and type names retain identifier segments, source spans, and pending
semantic annotations. `other::calculate` is an unresolved qualified identifier;
`object.field` is unresolved member access. Parsing does not decide which names
are files, aliases, declarations, locals, or valid members.

The active grammar includes unified `[pub] [const | comp]` bindings, including
`Name: type = TypeExpression;`, and ordinary functions. Parsing `comp` records a
modifier; it does not establish alias semantics. `const` records immutability and
is not an alias. Imports, labels and leading-`::` control-flow references are not
parsed; `pub fn` also needs a direct AST fixture today.

## Implemented lexical infrastructure

`SymbolTable` maintains a permanent root frame and explicitly entered scopes.
Lookup searches innermost to outermost. A frame rejects duplicate bindings in
that same frame without replacing the original identity. Exiting the root is an
error; exiting a nested scope removes its bindings from lookup.

Labels have a separate scoped table, so a label and lexical binding can share a
spelling. `LocalBinding` can hold a local identity or a compile-time alias entity;
multiple table entries can refer to the same entity. These operations are tested
as data-structure behavior, not produced by a completed source resolver.

In particular, the table itself does not enforce cross-frame redeclaration
policy, declaration order, capture boundaries, alias legality, or label-target
legality. Do not infer the full language shadowing policy from innermost lookup.

## Partial and unfinished project passes

`Analyzer::new` checks plan/unit ID agreement and creates a name registry;
`collect_declarations` creates per-file slots and rejects duplicates and invalid
top-level statements. Binding annotations remain pending. Name traversal handles
some expression forms and local lookup, but file fallback, qualified target
annotation/access checking, parameters, aliases and most statements are unfinished.
Local insertion currently precedes initializer traversal and does not fill the
binding annotation; this differs from the specification below.

`ProjectSymbols::from_plan` and `resolve_path`, type resolution and analysis
completion/initialization validation return not-implemented errors. Other pending
name branches may panic through `todo!()`. Parsing leaves annotations pending;
public semantic structures are not proof a completion pass has run.

The intended pass order is declaration collection, names/aliases, types/members,
then initialization and annotation validation. Driver frontend orchestration and
`AnalyzedProject` to IR lowering are also unimplemented. No successful parse
establishes a runnable or even type-correct program.

## Planned files, paths, and visibility

A source file is intended to own a namespace. The build-plan contract explicitly
enumerates named files; future path resolution must not discover extra files on
disk. `::` is intended for static selection through file namespaces or aliases;
functions and variables may be final declarations, not intermediate namespaces.
Locals are intended to take precedence for unqualified lookup, while a qualified
file path can bypass a same-spelled local.

File declarations are intended to be private by default, with `pub` exporting a
binding. Alias chains should preserve entity identity and obey access checks;
alias-only cycles should be rejected. Access checks and alias traversal are not
implemented.
Legacy include-path/import search is not an alternative implemented resolver;
see [Imports](parsing/imports.md).

## Intended `comp` versus `const`

The proposed `comp` binding aliases compile-time entities (files, functions,
file variables, or types), without separate runtime storage. An alias to a
mutable file variable is intended to denote the same storage. It is not a
promise of arbitrary compile-time expression evaluation. Local `comp` aliases
are intended not to target parameters or function-local variables.

The following parses today; its alias semantics remain unfinished:

```fib
comp File := other;
comp calculate := File::calculate;
comp counter := File::counter;
pub comp Color: type = struct { r: uint32, g: uint32, b: uint32 };
```

By contrast, parsed `const` declares a new binding with an immutability flag,
not an alias node. Intended const semantics copy an initializer value rather
than creating a live alias to variable storage; constness of a pointer binding
alone should not imply immutable pointee storage. Enforcement is pending.

First-class types and callable function values are design goals. Function values
are intended as plain function pointers without captured local state; APIs
needing callback state would take a separate context pointer with a caller-managed
lifetime. Nested no-capture function support remains undecided and is not parsed.

## Planned lexical policy and labels

The proposed policy rejects redeclaration of an enclosing parameter or local
within a function, allows reuse across sibling blocks/functions, and permits a
parameter to share a file declaration's spelling. Struct fields are separate
from lexical bindings. The current source resolver does not enforce this policy.

Named control-flow targets are intended to use a separate namespace, with
references beginning `::` (for example, design notation `break ::outer;`). Exact
label declaration syntax and permitted break/continue targets remain unspecified.
The parser currently accepts only bare `break;` and `continue;`.

## Intended local initializer visibility

The test specification resolves all initializer expressions before registering
any new local value bindings. In `x := x + 1;`, the RHS must refer to an existing
binding (for example, a file declaration); without one, it is unresolved. Later
uses refer to the new local. For `a, b := 1, a;`, the RHS `a` cannot refer to the
new `a`, because the entire shared initializer list precedes both new bindings.
The no-redeclaration rule still rejects reuse of enclosing parameters/locals.

This is a name-visibility rule, not execution during analysis. Compile-time alias
chains retain their separate forward-resolution contract.

## Planned forward references and initialization

File declaration collection predeclares names for forward references, unlike
local binding registration. File variable initialization is intended to run in
declaration order, rejecting reads of later uninitialized variables. This is
distinct from whether a name exists. Initialization validation is not implemented;
recursive type/value dependency rules still need definition.

Evidence: `src/frontend/namespace.rs`, `symbol_table.rs`, `scope.rs`,
`analyzer.rs`, and [`analyzer/test.rs`](../src/frontend/analyzer/test.rs). All
analyzer tests and helpers are in that single file; the inline `name_resolution`
module contains six entity-focused phases specifying identity and visibility
behavior using parsed source and isolated AST/table fixtures. They contain ordinary failing tests for unfinished behavior. The suite
does not run type checking, initialization validation, or compiled programs; see
[test commands and validation](agents/tests/README.md).
