# `src/ir/`

## Responsibility

The middle-end imports a supported subset of `TypedProgram` into flat
three-address-style control flow. Core entities are `Label`, `Temp`, `SymbolId`,
`Operand`, `Instruction`, `BasicBlock`, `IrFunction`, and `IrProgram`.

Files mirror concerns:

- `mod.rs`: representation, errors, shared numeric predicates, program import;
- `builder.rs`: scopes, IDs, instruction stream, block formation, defer/loop state;
- `expressions.rs`: typed expression import;
- `statements.rs`: functions, statements, control flow, and switch import;
- `display.rs`: debug text;
- `test.rs`: representation-shape tests.

## Representation Contract

- each source binding receives a `SymbolId`, preserving shadowing;
- source locals use explicit alloca/load/store operations;
- signedness and float category are carried by `Ty`;
- control flow is labels and jumps;
- `IfGoto` encodes only the true target; the false target is the next block;
- the builder splits blocks at labels and after selected terminators.

`IrProgram` currently omits nominal type definitions, module ownership, source
spans, target data, explicit CFG edges, and PHIs/block arguments.

## Effective Support

The importer supports scalar literals, locals, scalar arithmetic, logical
control flow, named calls, casts, loops, returns, defer, assignment forms, and
some enum/tuple operations. Aggregate, pointer, slice, enum payload, and
qualified operations are either rejected or represented by instructions the
LLVM consumer does not yet implement.

The existence of an `Instruction` variant does not imply end-to-end support.

## Known Correctness Risks

- short-circuit lowering assigns one `Temp` on multiple control-flow paths;
  the LLVM consumer stores only one host-side value and emits no PHI, violating
  dominance;
- pointer binary expressions can reach unchecked integer conversions in the
  consumer and panic rather than return unsupported;
- the false-edge-by-block-order convention is fragile and unverified;
- duplicate/missing labels, temp use-before-definition, dominance, operand
  categories, and terminator shape are not verified;
- normal nested-scope fallthrough does not emit deferred statements, diverging
  from direct lowering;
- constants are modeled/imported but ignored or broken in LLVM lowering;
- many emitted instruction variants are rejected by the consumer;
- route failure is hidden by broad driver fallback.

## Improvement Direction

1. Define an explicit capability result and permit fallback only for
   `UnsupportedFeature`.
2. Add structural verification before LLVM lowering.
3. Add explicit CFG successors and PHIs/block parameters, or define temporaries
   as mutable stack slots rather than claiming SSA-like values.
4. Carry nominal type/module/source information.
5. Delete dead variants or implement and test both importer and consumer.
6. Add a forced-route API and differential execution tests.
