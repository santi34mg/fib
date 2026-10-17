# Name resolution: implementation status and design

> **Boundary:** Tokenization/parsing and lexical table operations exist. The
> project name/type passes, file registration/path resolution, initialization
> validation, and lowering are explicit stubs. Design rules below are not
> enforced language behavior.

## What the parser records today

```fib
type Count int32
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

The active grammar includes `const` variable bindings, `type Name TypeExpression`,
and ordinary functions. It does **not** include `comp` bindings, `pub` declarations,
`import` declarations, labels, or leading-`::` control-flow references. `comp`,
`pub`, and `import` are currently ordinary identifier tokens, not active keywords.
A parsed `const File := other::file;` is not thereby a file alias.

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

## Unimplemented project passes

`ProjectSymbols::from_plan` and `resolve_path` return not-implemented errors.
So do `Analyzer::new`, `Analyzer::analyze`, declaration collection, name
resolution, type resolution, and analysis completion/initialization validation.
AST annotations remaining pending is intentional after parsing; public semantic
structures are not proof a completion pass has run.

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
alias-only cycles should be rejected. None of those rules currently runs.
Legacy include-path/import search is not an alternative implemented resolver;
see [Imports](parsing/imports.md).

## Planned `comp` versus parsed `const`

The proposed `comp` binding aliases compile-time entities (files, functions,
file variables, or types), without separate runtime storage. An alias to a
mutable file variable is intended to denote the same storage. It is not a
promise of arbitrary compile-time expression evaluation. Local `comp` aliases
are intended not to target parameters or function-local variables.

The following is **design notation, not accepted parser syntax**:

```text
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

## Planned forward references and initialization

Declaration collection is intended to allow forward name references within a
file. File variable initialization is intended to run in declaration order,
rejecting reads of later uninitialized variables. This is distinct from whether
a name exists. Those checks are not implemented; recursive type/value dependency
rules still need definition.

Evidence: `src/frontend/namespace.rs`, `symbol_table.rs`, `scope.rs`,
`analyzer.rs`, and `analyzer/test.rs`. Contract tests explicitly expect
unimplemented passes to fail rather than manufacture resolved annotations.
