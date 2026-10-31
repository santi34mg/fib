# Open design boundaries

The [language specification](../language-specification.md) defines file namespaces,
package-rooted paths, `root`, source roots/discovery, scoped visibility, protected
prelude type names, prefix arrays, lexical order, numeric/Boolean behavior,
mutability and exhaustive switches. The independently specified semantic features now execute. Recursive discovery
integration and ownership/lifetime enforcement still depend on open details; implementing
the other settled choices is not a new language-design question. Compare the implementation guides before claiming
support; the current compiler predates several decisions.

Remaining questions include:

- What exact source-discovery API shape, compiler namespace spelling and warning
  controls will expose the decided behavior?
- How will the existing `build() int32` and synthetic build API evolve to opaque
  values, recoverable errors, general interpretation and broader capabilities?
- How will broader value-returning compile-time computation extend the existing
  typed scaffolding and constant-data evaluator?
- What syntax will labels and constraints on ordinary metatype parameters have?
  How will nested no-capture functions extend existing lexical rules?
- What broader array-size expressions, ownership/lifetime rules, string APIs,
  overloading, interface constraints and external ABI policy will be adopted?
- How will source ranges/context and validation extend to remaining representations?

The implemented boundary remains original AST plus semantic annotations -> owned
SSA IR -> target-aware backend. Add positive/negative tests as confirmed changes
are implemented, and update implementation status alongside them. Unreviewed
proposals and current parser restrictions are not automatically language decisions.
