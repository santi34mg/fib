# Fib documentation

Fib is in the middle of a compiler refactor. **Intended language design and
current compiler behavior are different kinds of documentation.** A temporary
implementation limitation must not silently become a language rule.

## Language specification

The [language specification](language-specification.md) defines the settled rules
for file/package namespaces, visibility, prelude types, declarations, generics,
arrays, numeric behavior, mutability and enum switches. It describes the language
independently of current compiler support.

Implementation status and remaining design work are documented separately below.

## Verified implementation behavior

The [parsing reference](parsing/README.md) records accepted and rejected syntax,
examples, and parser-test evidence. These pages are an implementation snapshot,
not the language specification or a promise of semantic/runtime behavior.

The [compiler architecture](compiler-architecture.md) describes current data
contracts and unfinished pipeline boundaries. The
[agent implementation guide](agents/README.md) provides source-level details,
testing instructions, and contributor guidance.

## Language design and future work

- [Name resolution](name-resolution.md): current namespace, alias, visibility and
  lookup implementation, with links to the confirmed package design.
- [Type resolution](type-resolution.md): semantic rules implemented by the type
  pass and explicitly marked differences from the confirmed design.
- [Scalar semantics](scalar-semantics.md): implemented wrapping arithmetic, explicit
  conversions, runtime traps and IEEE rules, plus pending literal/Boolean cast changes.
- [Enum semantics](enum-semantics.md): executable constructors, payload copies,
  switches, storage and internal ABI/layout contracts.
- [Struct/storage semantics](struct-storage-semantics.md): implemented struct copies,
  partial initialization, typed pointers, null traps and unchecked places.
- [Roadmap](roadmap/README.md): proposals, dependencies, open decisions, and
  acceptance criteria. Proposed syntax is not necessarily parsed today.
- [Interpreted build files](roadmap/08-interpreted-build-files.md): future
  `build.fib` discovery, bootstrap lowering, IR interpretation and validated
  `BuildPlan` construction.

These design documents should guide the refactor; parser observations alone do
not settle language semantics. There is not yet a complete standalone language
specification in this tree.

## Analysis work

Analysis gaps discovered while checking the documentation belong in actionable
GitHub issues, not repeated warnings in user-facing syntax pages. The
[analysis issue inventory](agents/issues/README.md) links the six published reports
(#9–#14). Track implementation progress and discussion on
GitHub; these reports do not settle the open language-design questions in RFC #2.

The [samples](../samples/README.md) use current syntax and all have automatic
parser coverage. Selected scalar, loop, struct and enum samples also have
compile/link/run coverage in the executable acceptance suite; this does not
certify every sample as executable. Standard-library implementations remain pending;
the historical source sketches and C bindings have been removed.
