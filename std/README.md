# Fib Standard Library (std)

The standard library for the Fib language, imported with `import std::...`
and resolved via `-I=std` (the entry file's directory is always searched
first, `-I` adds more). Layout mirrors the import paths: `std/io/fd.fib`
is `std::io::fd`.

Status: early scaffolding. `libc.fib` is the only fully usable module today
(every sample builds on it); `core`/`fs`/`io` are typed stubs being fleshed
out as the language grows.

## Files

- `libc.fib` (`std::libc`) — `extern` C bindings: `printf`, `malloc`/`free`,
  `memcpy`, `exit`, `strlen`, `puts`.
- `core/error.fib` (`std::core::error`) — `Error { message: @string, code: @int }` stub.
- `fs/path.fib` (`std::fs::path`) — `Path` wrapper with
  `path_from_string`/`path_as_string` helpers.
- `io/fd.fib` (`std::io::fd`) — `FileDescriptor` placeholder struct.
- `io/stream.fib` (`std::io::stream`) — re-exports `Error`/`Path` for future stream APIs.
