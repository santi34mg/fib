# 05 — Redesign strings and the C boundary

**Status:** future representation proposal. **Related:**
[generic data types](04-generic-types.md), [builtin cleanup](01-builtin-sigil.md).

## Current baseline versus history

The tokenizer/parser retain `string`, string/character literals and builtin
string calls. No semantic literal typing, allocation, NUL layout or LLVM string
emission executes. The old opaque-pointer/NUL implementation, pointer equality
bug and fixed 64-bit slice/payload layouts are historical motivation, not
verified current runtime behavior. `c"..."` is not implemented C-string syntax.

## Retained proposal

| Proposed form | Intended meaning | Decisions still required |
| --- | --- | --- |
| `"hola"` | Fixed byte/character array, e.g. `[4]char` | Encoding, length unit, mutability and storage lifetime |
| `c"hola"` | NUL-terminated storage addressed as `*char` | Embedded NUL, encoding, FFI ownership and escapes |
| `[]char` | Borrowed view | Lifetime, mutation, target-width length/layout |
| `Vec[char]` | Owning generic buffer | Allocation failure, destruction and explicit C conversion |

All prefix/generic forms in the table are proposals. Do not promise that
`char` has a working i8 runtime representation or that Unicode escapes fit one
byte without an explicit encoding policy. Do not hard-code `{ptr, i64}` or
aggregate sizes: target layout and ABI work are prerequisites.

## Work

- [ ] Specify literal encoding/byte length, global storage and array-to-view
      conversions, including unequal-length literal branches such as true/false.
- [ ] Decide whether to remove `BuiltinType::String`, provide a migration period,
      or retain a distinct language string abstraction.
- [ ] Add a dedicated C-string token/AST form and diagnostics only after its
      grammar and embedded-NUL policy are settled.
- [ ] Audit each std FFI declaration against the C API: inputs, output buffers,
      pointer indirection, const/mutation, ownership and returned lifetime.
      Historical token/declaration counts and opaque-pointer behavior are not
      proof that declarations are correct.
- [ ] Provide owning/view APIs and explicit runtime-built NUL conversion before
      migrating std, format strings, samples and builtin calls.
- [ ] Decide array/slice equality: reject with a directed library hint or define
      content equality explicitly. Never accidentally use pointer equality or
      attempt scalar LLVM conversion of aggregates.
- [ ] Validate enum payload layout/alignment and every affected call/return ABI
      through SSA and target-aware backend plans.

## Acceptance tests

Tokenizer/parser malformed and successful C-string forms; encoding and embedded
NUL; literal length and view lifetime; explicit C conversion and allocation
failure; equality diagnostics; target-dependent aggregate layouts. Add actual
printf/link/run tests only once a harness and backend exist. Current samples
and stub tests do not validate this proposal.
