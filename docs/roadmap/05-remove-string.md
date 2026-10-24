# 05 — Redesign strings and the C boundary

**Status:** future representation proposal. **Related:**
[metatype specialization](../language-specification.md#declarations-generics-and-lexical-scope), [builtin cleanup](01-builtin-sigil.md).

## Current baseline versus history

The tokenizer/parser retain `string` and string/character literals. String
helpers are ordinary identifiers, not builtin calls. Semantic literal typing
exists, and literal strings reach the
narrow build API as interpreter-owned bytes. General runtime string allocation,
NUL layout and LLVM string emission remain unfinished. The old opaque-pointer/NUL
implementation, pointer equality bug and fixed 64-bit slice/payload layouts are
historical motivation, not
verified current runtime behavior. `c"..."` is not implemented C-string syntax.

## Retained proposal

| Proposed form | Intended meaning | Decisions still required |
| --- | --- | --- |
| `"hola"` | Fixed byte/character array, e.g. `char[4]` | Encoding, length unit, mutability and storage lifetime |
| `c"hola"` | NUL-terminated storage addressed as `*char` | Embedded NUL, encoding, FFI ownership and escapes |
| `char[]` | Borrowed view | Lifetime, mutation, target-width length/layout |
| Metatype-based owning buffer | Owning generic buffer | Allocation failure, destruction and explicit C conversion |

The literal representations, view ownership and buffer API in the table remain
proposals; array/slice types use suffix syntax. Generic library types use
metatype parameters and type
computation. Do not promise that `char` has a working i8 runtime representation or that Unicode escapes fit one
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
      migrating std, format strings, samples and ordinary string helper calls.
- [ ] Decide array/slice equality: reject with a directed library hint or define
      content equality explicitly. Never accidentally use pointer equality or
      attempt scalar LLVM conversion of aggregates.
- [ ] Extend existing enum payload layout/alignment and internal ABI coverage to
      the chosen string/view representations and their C boundaries.

## Acceptance tests

Tokenizer/parser malformed and successful C-string forms; encoding and embedded
NUL; literal length and view lifetime; explicit C conversion and allocation
failure; equality diagnostics; target-dependent aggregate layouts. Add actual
printf/link/run tests through the existing harness once string lowering and
external C interoperability exist. Current samples
and stub tests do not validate this proposal.
