# Fib documentation

Fib is in the middle of a compiler refactor. **Intended language design and
current compiler behavior are different kinds of documentation.** A temporary
implementation limitation must not silently become a language rule.

## Verified implementation behavior

The [parsing reference](parsing/README.md) records accepted and rejected syntax,
examples, and parser-test evidence. These pages are an implementation snapshot,
not the language specification or a promise of semantic/runtime behavior.

The [compiler architecture](compiler-architecture.md) describes current data
contracts and unfinished pipeline boundaries. The
[agent implementation guide](agents/README.md) provides source-level details,
testing instructions, and contributor guidance.

## Language design and future work

- [Name resolution](name-resolution.md): namespace, alias, visibility, and lookup
  design, with its implementation status explicitly identified.
- [Type resolution](type-resolution.md): semantic rules implemented by the type
  pass and the boundary with unfinished full analysis.
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

The [samples](../samples/README.md) use current syntax and are covered by an
automatic parsing test, not an executable conformance suite. The
[standard library](../std/README.md) still contains historical source sketches.
