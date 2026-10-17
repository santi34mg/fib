# Open design boundaries

These are implementation questions, not completed features. Compare source and
[parser behavior](src/frontend/parser.md) before extending the language.

- How will `build.fib` interpretation construct named plans and report source errors?
- How will declaration collection reconcile forward visibility with initialization order?
- How will file visibility, aliases, cycles, no-capture boundaries and generic identities
  be validated without filesystem discovery during semantic resolution?
- What syntax will labels and generic parameters have? Current reserved annotation
  slots/string lists do not define that syntax.
- How will source records/ranges survive analysis and SSA lowering for diagnostics?
- How will annotation completion, SSA dominance/edges and target/ABI legality be
  verified before emission can return an artifact?

The intended boundary remains original AST plus semantic annotations -> owned SSA
IR -> target-aware backend. A second typed AST or silent direct-emission fallback
would contradict the checked-in sketches. Resolve questions with explicit tests
and update the relevant implementation page when behavior becomes real.
