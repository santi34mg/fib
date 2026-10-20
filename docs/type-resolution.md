# Type resolution

The implemented sequence is `Analyzer::new` → `collect_declarations` →
`resolve_names` → `resolve_types`. Each stage runs once; a failed pass cannot be
retried against partial state. `resolve_types` returns the project's `SemanticTypes`
and annotates the original source AST, available through `Analyzer::units`.
The free `resolve_types` entry requires collected, name-resolved units and a fresh
type workspace. No file discovery occurs.

This pass is separate from complete analysis: `Analyzer::analyze` runs typing,
then reports that initialization and complete flow checks are unfinished. The pipeline, IR and backend remain unfinished.

## Semantic identities

Structs and enums are nominal per definition. Two same-shaped definitions are
distinct; aliases preserve the original identity. Builtins, pointers, arrays,
slices, tuples and function signatures are canonicalized structurally.

`SemanticTypes` separates binding value types/categories from the types denoted
by metatype values. A binding containing a type has value type `type` and a separate
`type_value`. Fields and variants are identified by owner and source-order index;
they never become lexical bindings. Enum payload fields have their own aggregate
owner. Internal pending reservations must all be finalized before success.

Named definitions and function signatures are resolved before bodies. Types may
recur through pointers, slices or function values. Cycles through by-value fields,
payloads, tuples or arrays are rejected, including zero-length arrays. Local type
visibility continues to follow the name pass's declaration-order rules.

## Resolution versus compatibility

`resolve_types` orchestrates the project. Per-node resolvers infer types, propagate
expected types, check literal ranges and operand domains, assign value/place
categories, validate arities and write annotations with source diagnostics.

`check_assignable(expected, found, types)` only compares already resolved semantic
types after canonicalization. It is read-only, validates referenced type IDs, and
respects nominal aggregate identities. It does not infer types, inspect syntax,
check literal ranges, authorize casts, validate places or unpack tuples.

For example, `small: int8 = 300;` fails while contextualizing its literal;
`small: int8 = existing_int32;` fails compatibility checking. Assignment validates
its mutable place before compatibility; calls validate callable type and arity
before checking arguments; casts use a separate explicit-conversion matrix.

`TypeContext::new` supports lower-level node entrypoints with an already prepared
symbol/type environment. These helpers do not certify project completion.
`validate_type_annotations` checks the complete type-pass handoff independently:
applicable names/bindings/types/expressions/members, semantic-table consistency,
project identities and variant constructor signatures. It neither fills missing
slots nor validates initialization/control flow. File namespaces have no fabricated
runtime expression types. Reserved label syntax has no applicable slots.

## Values and special types

- `void` means no returned value in function signatures; elsewhere it is unit.
- `*void` is an opaque raw pointer. Opacity is queried centrally so future opaque
  types can extend the same boundary. Dereference/member/index operations on it
  are rejected.
- `never` is currently admitted only as a return type.
- `type` is admitted in `comp` bindings, function parameters and returns. Signature
  references to metatype parameters use symbolic identities. Bodies such as
  `fn identity(T: type, x: T) T { return x; }` can be checked symbolically;
  Calls requiring compile-time evaluation or specialization report errors.
- `usize` remains symbolic, independent of the host's pointer width. It is the
  `.len` result for arrays, slices and strings (byte count for strings). A struct
  field named `len` remains an ordinary field.

Parameters and runtime bindings carry semantic types. Explicit types are available
independently of initialization. File dependency inference may resolve forward
references; it does not authorize reads of uninitialized variables. Inference
cycles without sufficient explicit types fail. Multi-bindings retain independent
modifiers and types; one tuple-valued initializer can supply multiple destinations.

## Expressions

Numeric literals default to `int32`/`float64`, adopting a numeric context when
representable. Unary negative integer boundaries are checked after the sign.
Already typed values require matching types or explicit numeric casts; no implicit
mixed-width/signedness promotion occurs. Tokenizer `u64`/`f64` payload limits remain.
Syntactic constants are checked for representable arithmetic and casts, division
by zero and invalid shifts. Integer magnitude covers `uint128`; floating checks
use an extended exponent instead of the host `f64` range. They retain the
payload's floating precision and are not a general compile-time evaluator or an
exact IEEE arithmetic interpreter. Target-width `usize` validation is deferred.

Arithmetic accepts numeric operands, remainder/bitwise operations require
integers, logical operators require booleans, and ordering accepts numbers or
characters. Shifts retain the left type. Equality accepts scalar/string/unit
values and same-typed pointers; aggregates and functions are rejected.

Assignments require mutable places. Fields/array indices inherit receiver
categories; pointer dereferences and slice elements refer to mutable storage even
when the pointer/slice binding is const. Addressing immutable places is rejected
until an appropriate pointer type exists. Raw-pointer checks promise no lifetime
or ownership safety.

Pointers support dereference, same-type equality and explicit `*T` ↔ `*void` casts.
Pointer indexing/slicing/arithmetic/ordering, other pointer casts and pointer/integer
casts are rejected. `null` requires pointer context. Numeric casts are explicit;
aggregate, enum/integer and array/slice conversions are rejected.

Ordinary, forward, recursive and indirect calls check callee type, argument count,
argument types and result type. Returns check the containing signature and result
arity/types; path completeness remains separate. Variadic typing explicitly errors.
There are no builtin functions: string helpers will be ordinary standard-library
functions; the compiler defines no signatures or dispatch by their spelling.

Struct construction requires every field once, with no defaults. Payloadless enum
variants are values; payload variants are positional constructors in declaration
order, including zero-argument constructors for explicitly empty payloads.
Arrays are homogeneous and context checks lengths; empty arrays require context.
Indices and slice bounds must be integers. Known array bounds, reversed ranges and
invalid inclusive bounds are diagnosed; dynamic bounds checks are not emitted.
Pattern payload bindings are typed and scoped to their arms; conditions are bool.
Expression inference currently rejects nesting beyond 256 recursive expression
calls with a diagnostic, rather than promising unbounded analysis resources.

## Acceptance coverage

Type-only acceptance tests cover semantic identities, members, annotations and
source diagnostics. Initialization, flow, specialization and type-return evaluation
remain separate work; `Analyzer::analyze` cannot yet return a complete project.
