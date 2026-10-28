# Fib language specification

This specification defines the settled language rules. Compiler implementation
status is documented separately in the [implementation guide](agents/README.md)
and [parsing reference](parsing/README.md).

## Names, packages and source discovery

- Every file is a declaration namespace. Directories organize paths and
  visibility regions; they do not contain source declarations of their own.
- Every package has an explicit source root. Fully qualified paths have the form
  `Package::directory::file::binding`, relative to that source root, omitting the
  `.fib` extension. Discovering a smaller subtree does not change these paths.
- `root` names the package containing the source, including inside dependencies;
  it never means the consuming application from dependency code. Dependency names
  resolve in that package's own declared dependency environment.
- Package names and `root` cannot be shadowed. Explicit namespace aliases can
  shorten paths, for example `comp vectors := root::math::vector;`, followed by
  `vectors::normalize(value)`. Aliases preserve canonical target identities.
- Package, directory and file segments are case-insensitive. Declaration names,
  fields, variants, aliases and local bindings remain case-sensitive. Hyphens in
  package/directory/file segments become underscores; spaces and language keywords
  are invalid. Collisions after normalization, including case-only differences
  and file/directory conflicts, are compile-time errors.
- A build-API operation recursively discovers `.fib` files beneath a requested
  directory within the source root. It skips symlinks, excludes the active build
  file and supports explicit exclusions; hidden, test and build directories are
  not automatically excluded by convention.
- The final selected source tree defines namespace collisions. Overlapping
  discovery calls and explicit additions include each file once. Discovery
  materializes a deterministic build plan before source lookup; lookup never
  discovers additional files on demand.

| Visibility | Accessible from |
| --- | --- |
| No modifier | Defining file |
| `pub(dir)` | Containing directory and its subtree, within the same package |
| `pub(root)` | Any file in the same package |
| `pub` | Other packages that can name the exporting package |

A public or less-restricted alias may re-export a more restricted declaration if its author can access the target. Access through the
alias follows the alias's visibility; the original path retains its restrictions.
This does not introduce the dependency into a consumer's package-name environment.

Primitive type names, including `type`, are ordinary identifiers resolved to
compiler-owned type values supplied by an implicit prelude. Prelude type names
cannot be shadowed. Primitive internal identities still determine arithmetic and
representation; token spelling does not.

## Declarations, generics and lexical scope

Unified bindings use inferred or explicit types, with independent per-binding
modifiers: `x := 1;`, `const x: int32 = 1;`, `comp T: type = int32;`.
Type bindings require `comp`. Ordinary `T: type` parameters and type-value arguments provide specialization; eligible `comp fn`
functions perform bounded type computation.

Array/slice type syntax is suffix-based: `T[N]` and `T[]`.
In expression positions, semantic resolution of the subject must distinguish array-type construction from value indexing; primitive names
receive no special lexical treatment.

Active enclosing locals and parameters cannot be shadowed. Locals may reuse
file-level declaration names; sibling scopes may reuse names. Protected prelude
and package names are exceptions. File declarations are forward-visible, while
locals enter scope after their initializers resolve. Every initializer in a
multi-binding resolves before any new binding is introduced. Thus an initializer
using the new binding's spelling refers to an existing outer/file binding or fails
if none exists. Name visibility does not authorize uninitialized reads.

## Numeric and Boolean behavior

Typed numeric values never convert implicitly. Literals adopt representable
numeric context, otherwise defaulting to `int32` or `float64`; `int` and `uint`
are aliases for `int32` and `uint32`. Operand types determine arithmetic width,
not a wider destination or build mode. Constant evaluation shares runtime rules.

Integer arithmetic wraps at its width. Division/remainder by zero and negative
or oversized shift counts trap; known invalid operations are compile-time errors.
Signed division truncates toward zero; remainder has the dividend's sign.
`MIN / -1` wraps to `MIN`, and `MIN % -1` is zero. Integer casts retain low bits
when narrowing and extend according to source signedness when widening.

IEEE binary16/32/64/128 arithmetic uses round-to-nearest, ties-to-even, gradual
underflow, signed zero, infinity and NaN, without implicit fast-math or fused
contraction. Arithmetic overflow produces infinity; overflowing literals are
errors. Floating division by zero follows IEEE. NaN compares unequal, including
to itself, and ordering comparisons with NaN are false; payload propagation is
unspecified. Numeric conversions may lose precision. Float-to-integer conversion
truncates toward zero, then traps on an unrepresentable result, infinity or NaN.

Direct same-family numeric literals receive an `as` destination's context,
including parentheses and unary minus. Integer literals use integer destination
context; float literals use float destination context. Values must fit:

```fib
4294967296 as uint64;  // valid uint64 literal
0.1 as float32;       // round directly to float32
300 as uint8;         // error: literal out of range
(-1) as uint8;        // error: negative unsigned literal
```

Typed operands, arithmetic expressions and cross-family casts retain conversion
semantics. For example, an `int32` variable holding 300 casts to `uint8` as 44;
`(2147483647 + 1) as int64` wraps the int32 addition before converting, and
`3.9 as int32` truncates to 3.

Explicit numeric-to-bool casts yield false for zero and true otherwise. Both
floating signed zeros are false; nonzero values, infinities and NaN are true.
Explicit bool-to-numeric casts produce 0 or 1. Conditions and logical operators
require bool: there is no implicit numeric truthiness. `&&` and `||` evaluate
left-to-right and short-circuit.

## Values, initialization and mutability

Structs and enums are nominal; aliases preserve identity. Struct construction
requires every field exactly once, with no implicit defaults. Field initializer
expressions execute in written order; field storage follows declaration order.
Plain-data assignment, arguments and returns copy values, including nested
values; copied pointers preserve their addresses and therefore pointee aliases.

Uninitialized declarations and field-by-field initialization are allowed. Every
read must be definitely initialized; copying a whole struct requires every field.
There is no implicit zeroing. Loops that may run zero times do not establish
initialization after the loop. Assignment evaluates the RHS first, then evaluates
the destination once, then writes.

Binding and pointee/element mutability are independent and layer-local:

| Form | Binding/whole-value replacement | Pointee/element writes |
| --- | --- | --- |
| `const p: *T` | Forbidden | Allowed |
| `p: const *T` | Allowed | Forbidden |
| `const arr: T[N]` | Forbidden | Allowed |
| `arr: const T[N]` | Allowed | Forbidden |

Fixed arrays are inline values: whole-array assignment replaces the value in the
same storage, not an address/reference. The same binding-versus-element separation
applies to slices. Qualifiers do not deeply freeze nested pointers/containers.
Writable pointers and slices implicitly convert to readonly views of the same
underlying type; reverse conversion and readonly-erasing casts are forbidden.
Nested pointee/element types must match exactly; no unsafe deep qualifier change
is implied. This rule does not introduce an array-to-slice conversion.

Pointer types use `*T`, address-taking uses postfix `value&`, and explicit
dereference uses postfix `pointer*`. `pointer.field` may insert exactly one
implicit dereference per field access and emits a warning; a second pointer layer
requires explicit dereference. Inserted dereferences preserve readonly permissions
and ordinary null checks. Ordinary dereference traps on null;
`std::memory::unchecked_deref(pointer)` skips only the selected null check. Neither
form guarantees lifetime, allocation validity or ownership safety.

## Enums and switches

Variant tags are zero-based in declaration order. A variant is declared as `Name`
or `Name: Type`, where the payload expression must denote a storable type.
Non-unit variants are one-argument constructors (`E.Number(42)`). Struct payloads
use an explicit struct type and take one struct value, for example
`Pair: PairType` and `E.Pair(PairType { left: 1, right: 2 })`.
The former `Name { field: Type }` declaration syntax is removed.

An omitted payload, `void`, an empty struct, or a struct composed recursively
only of these unit types normalizes to no payload data. Aliases are followed and
generic payloads are classified after specialization. Such variants are values
(`E.Empty`), and calls such as `E.Empty()` are rejected. Other zero-sized types,
including zero-length arrays, retain their payloads. Normalization does not merge
distinct variant tags or change nominal type identity.

A switch evaluates its subject once; an optional arm binding is a mutable copy
of the single payload value, scoped to that arm, with pointer aliases preserved.
Every normalized unit variant permits an optional binding of type `void`, even
if its declared payload was a unit struct. Its original struct fields are not
members of that binding. Payload construction evaluates its argument once.

Every enum switch must be exhaustive: list every variant or cover the remainder
with `when else`. Missing coverage is a compile-time error, with no implicit
unmatched fallthrough. An arm may complete normally and continue after the switch.
Duplicate variants and arms after a wildcard are compile-time errors. Break/continue target
an enclosing loop, not the switch.
