# Experimental standard-library sources

[`std/`](../../../std) contains libc/FFI declarations and experimental collections,
memory, I/O, threading, networking and other APIs. Presence is not proof of
implementation, safety, portability or test coverage. There is no working import
loader or project compiler that frontend-checks or runs this tree, and no std
sweep in CI. Samples retain qualified `libc::` calls without import declarations;
their parser test does not load or check the historical libc bindings.

Variadic libc declarations (`printf`, `snprintf`, `sscanf`, `sprintf`, `fprintf`,
`open`, `fcntl`, `sem_open`) have been removed along with variadic language support.
Historical callers are not migrated; these APIs need fixed-signature C shims
before they can be exposed again. Do not replace them with incorrect fixed-arity
prototypes for the original C symbols.

The former `std/string.fib` wrapper around `@concat` is removed; its library
implementation remains pending. Sources can retain obsolete syntax. Cross-check with the [parser](../src/frontend/parser.md)
before interpreting any failure as a semantic/library bug. No currently supported
linker-selection, native-library flags or installed-stdlib discovery is provided.

Before promoting any module, inspect its actual implementation and record:
syntax/analysis status, implemented vs stub APIs, platform/native libraries,
FFI sizes/layouts, ownership/allocation contracts, and deterministic runtime tests.
Handwritten libc/pthread declarations, raw pointers, fixed buffers and guessed
opaque-object sizes require particular review. Do not carry forward old module
safety audits as freshly verified facts. Placeholder operations should fail
explicitly rather than pretend success when those APIs are implemented/tested.
