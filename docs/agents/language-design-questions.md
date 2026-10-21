# Open design boundaries

These are implementation questions, not completed features. Compare source and
[parser behavior](src/frontend/parser.md) before extending the language.

- Which source signature, namespace mechanism, opaque value representations and
  recoverable-error model will expose the decided interpreted `build.fib` API? The
  execution route itself is fixed in the
  [future plan](../roadmap/08-interpreted-build-files.md): verified owned IR,
  host-owned builder state and no native build-file loading.
- How will declaration collection reconcile forward visibility with initialization order?
- How will nested no-capture functions and generic identities extend the existing
  file visibility, alias-cycle, and lexical name-resolution rules?
- What syntax will labels and generic parameters have? Current reserved annotation
  slots/string lists do not define that syntax.
- How will source records/ranges survive analysis and SSA lowering for diagnostics?
- How will annotation completion, SSA dominance/edges and target/ABI legality be
  verified before emission can return an artifact?

The intended boundary remains original AST plus semantic annotations -> owned SSA
IR -> target-aware backend. A second typed AST or silent direct-emission fallback
would contradict the checked-in sketches. Resolve questions with explicit tests
and update the relevant implementation page when behavior becomes real.
