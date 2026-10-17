# Self-owned SSA IR sketch

[`src/ir/mod.rs`](../../../../src/ir/mod.rs) exposes `Ir` with owned arenas for
functions, globals, types, constants, blocks, values and stack slots. Arena IDs
are IR-wide indices, not frontend IDs or source spellings. Records/accessors and
diagnostic formatting work; arbitrary public records remain unverified.

## Source map

- `mod.rs`: arenas, IDs, functions/globals/storage, accessors and `IrError`.
- `types.rs`: owned runtime types, signatures and calling conventions.
- `expressions.rs`: values/definitions, constants, operands and addresses.
- `instruction.rs`: instruction/operator records.
- `statements.rs`: blocks, explicit edges/arguments, terminators and switch cases.
- `builder.rs`: `FunctionBuilder` API; every operation returns NotImplemented.
- `verify.rs`: `verify_ir` always returns NotImplemented, even for empty IR.
- `display.rs`: diagnostic display, not validation or serialization certification.
- `test.rs`: raw representation/access/formatting and explicit-failure contracts.

## Intended invariants, not established behavior

SSA values have unique definitions; block parameters and explicit edge arguments
represent merges. There is no implicit block-order fallthrough. Mutable storage
is separate from SSA values. Function-local records have owner metadata; entry
parameters match signatures and edges cannot target entry. Declarations have no
body. Future verification must check references, ownership, definitions/dominance,
edge types/arity, instructions, memory operands and calls/returns.

The old `IrProgram` flat importer, mutable `Temp` model and typed-program import
are not active APIs. `driver::lower_to_ir` is the sole intended frontend boundary
and errors today. No optimization, executable interpreter, verified construction,
or end-to-end backend consumer exists. Never interpret successful formatting or
an empty arena as verified IR.
