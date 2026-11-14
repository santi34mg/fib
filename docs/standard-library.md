# Standard-library foundation

`std` is a version-matched source dependency installed with the compiler. Its
files use the same package lookup, visibility and alias rules as application
dependencies. Ordinary Fib functions and compiler intrinsics can share a file
namespace; compiler operations are selected by canonical declaration identity.

`core::sizeof(T)` is a compiler intrinsic available with `--no-std` as well.
It is exposed only in the compiler-owned `core` namespace. It accepts
one type value and returns `usize`, measuring target allocation size in bytes
(including padding and array stride). Empty structs and zero-length arrays have
size zero; void, never and metatypes are unsized and rejected. Generic type
arguments are measured after specialization. LLVM resolves the size using the
selected target; frontend constant evaluation does not yet support it in `comp`
values, array lengths or global initializers. Compiler-declaration revision 6
adds per-target source selection in `sysroot.toml` and the macOS std (revision 5
added the package-private `std::os_abi::terminated_from_parts` intrinsic; revision 4
recorded prefix container syntax, sentinel byte types and the terminated `openat`
path contract); reinstall sources alongside the compiler.

## Targets

Programs build for the host. Two targets run with the full std:

| Target | C ABI | `std::os` implementation |
|---|---|---|
| Linux x86-64 | System V | `os.fib`: typed wrappers over raw syscalls (`std::syscall` numbers) |
| macOS arm64 (Apple Silicon) | AAPCS64 (Apple) | `darwin/os.fib`: the same API over libSystem (`darwin/libc.fib`) |

x86-64 macOS and arm64 Linux have C ABI support, so `--no-std` programs and
C interop work there, but no std `os` implementation yet. On Apple arm64,
float128 arithmetic, comparisons and conversions are rejected at compile time:
its runtime has no soft-float float128 routines. Storing and copying float128
values still works.

`sysroot.toml` lists shared logical sources and per-OS-family adjustments:

```toml
[targets.linux]
exclude = ["libc.fib"]

[targets.darwin]
exclude = ["syscall.fib"]
replace = { "libc.fib" = "darwin/libc.fib", "os.fib" = "darwin/os.fib" }
```

`replace` changes the file behind a logical source while its module path stays
the same (`darwin/os.fib` is still `std::os`); `exclude` removes modules a
target lacks. The family is the compiler host's (`linux`, `darwin`), or
`StandardLibrary::target_os` for host API callers. Opening a target-specific std
file in the editor selects a target that loads it, so every std file can be
edited from any host.

The darwin `std::os` keeps the Linux names, `Errno`/`Result` aliases and a
Linux-shaped `Stat`, with darwin constant and errno values (for example
`MAP_ANONYMOUS = 0x1000`, `AT_FDCWD = -2`, `EAGAIN = 35`). Differences:

- The `syscall0`–`syscall6` intrinsics are rejected by the backend and
  `std::syscall` is not loaded. Linux-only operations and flags (`getdents64`,
  `accept4`, `sbrk`, `O_DIRECT`, `O_PATH`, `CLOCK_BOOTTIME`, `SOCK_CLOEXEC`, …)
  are absent, so using them fails at name resolution.
- `pipe2` and `dup3` apply `O_CLOEXEC`/`O_NONBLOCK` with `fcntl` after the call,
  which is not atomic.
- `getrandom` fills at most 256 bytes per call (`getentropy`) and ignores flags.
  `random::fill` loops, so it fills buffers of any size.
- `Utsname` has darwin's five 256-byte fields. `exit` uses `_exit`.
- The heap allocator takes fresh storage from private mappings instead of the
  program break.
- Variadic C functions (`openat`, `fcntl`, `ioctl`) are declared with padding
  arguments so the variadic value lands in the first stack slot, where Apple
  arm64 passes variadic arguments.

Install the binaries and sources together:

```sh
cargo run --no-default-features --bin fib-install -- --prefix "$HOME/.local"
```

The layout is `bin/{fibc,fib-lsp}` and `lib/fib/<version>/{sysroot.toml,std/}`.
The Rust installer builds release binaries by default; pass `--debug` for debug
binaries. It obtains executable paths from Cargo's build messages, including when
a custom target directory is configured. The installer itself needs no LLVM to
compile; building the compiler binaries still requires LLVM 21.
The manifest records the compiler version, compiler-declaration revision and
explicit source list. Missing or incompatible installations are errors. Source
loading is deterministic and completes before parsing or name lookup.

For development:

```sh
cargo run -- build --sysroot library --single-file samples/syscalls.fib
FIB_SYSROOT="$PWD/library" cargo run --no-default-features --bin fib-lsp
```

Both binaries accept `--sysroot` and `--no-std` (`fibc` accepts them on `build`).
The sysroot comes from the explicit option, then `FIB_SYSROOT`, then the
executable's installation prefix. No-std mode omits the library dependency and
library prelude; primitive types remain available. Cargo installation alone does
not copy the library; use the installation script or provide a sysroot explicitly.
Host API callers configure `BuildPlan::standard_library` or
`PlanBuilder::with_standard_library`. `BuildPlan::prepare` returns an idempotent
complete plan without modifying its input or changing existing file identities.
Low-level `Analyzer` callers supply the prepared plan and its parsed units.

Opening a std source file directly uses its owning `sysroot.toml` and explicit
source list, including when editing a checkout with an installed language server.
The document keeps its file ID and belongs to the std package, so `root::result`
and package-private helpers resolve normally. Other open std files supply unsaved
overlays in that same package. An enclosing application's `build.fib` does not
select the library's source membership. `--no-std` still disables this preparation.

## Module overview

| Module | Contents |
|---|---|
| `result` | `Result(T, E)` and helpers (`is_ok`, `unwrap_or`, `expect`, `map`, …) |
| `option` | `Option(T)` and helpers (`is_some`, `unwrap_or`, `expect`, `ok_or`, `map`) |
| `memory` | intrinsics, `Allocator`, heap/page allocators, `create`/`destroy`, `alloc`/`free`, `span` |
| `vector` | `Vec(T)`, a growable array over an `Allocator` |
| `map` | `Map(K, V)` with caller-supplied equality |
| `string` | byte-string helpers: `eq`, `index_of`, `concat` |
| `io` | `Reader`/`Writer` interfaces, `read_full`, `write_all`, `print`, `printf` |
| `formatting` | `Arg`, `format`, `format_to` |
| `os` | OS wrappers, `Errno`, constants (Linux syscalls; macOS libSystem) |
| `syscall` | every Linux x86-64 syscall number (Linux only) |
| `process` | `Arguments`, `Environment`, `getenv`, `abort` |
| `random` | OS-backed randomness |

Every fallible operation returns a `Result` (or an `Option` for plain absence);
none signals failure through an empty value or a boolean. Generic helpers take
their type arguments explicitly: `std::result::unwrap_or(int32, E, value, 0)`.

## Input and output streams

`std::io::Reader` and `std::io::Writer` are callback structs, like `Allocator`:

```fib
pub comp Reader := struct { context: *void, read: fn(*void, []uint8) CountResult };
pub comp Writer := struct { context: *void, write: fn(*void, const []uint8) CountResult };
```

`CountResult` is `std::os::CountResult` (`Result(usize, Errno)`). A `read`
stores up to `buffer.len` bytes into a prefix and returns how many; zero for a
nonempty buffer means end of input. A `write` consumes a prefix and returns its
length. Either may make partial progress or fail with `EINTR`.

- `fd_reader(fd: *int32)`/`fd_writer(fd: *int32)` adapt a descriptor. They
  borrow `fd`'s storage, which must stay live and unchanged; they never close it.
- `stdin()`, `stdout()` and `stderr()` return adapters over program-lifetime
  descriptor storage.
- `read_full(reader, buffer)` reads until the buffer is full or input ends,
  retrying `EINTR` and short reads; a count below `buffer.len` means end of input.
- `write_all(writer, bytes)` writes every byte, retrying `EINTR` and short
  writes; zero progress reports `EIO`.

Empty buffers make no call. Errors return the `Errno` and may leave partial
transfers. These two loops are the only retry loops in std: `print`/`printf`
and `random::fill` use them.

```fib
comp Counter := struct { bytes: usize };
fn count(context: *void, bytes: const []uint8) std::io::CountResult {
    counter := context as *Counter;
    counter.bytes = counter.bytes + bytes.len;
    return std::io::CountResult.Ok(bytes.len);
}
fn main() int32 {
    counter := Counter { bytes: 0 };
    sink := std::io::Writer { context: (counter&) as *void, write: count };
    std::io::writef(sink, "{} items", [std::io::Arg.Unsigned(12)]);
    return counter.bytes as int32; // 8
}
```

## Process entry and maps

On every supported target, `main` may return `int32`, `void`, or an instance of
`std::result::Result(T, E)` with `T` equal to `int32` or a unit type. A void return
or unit `Ok` exits with status 0; `int32` and `Ok(int32)` supply the exit status.
Every `Err` exits with status 1; its payload is discarded without printing.
The host applies its usual exit-status truncation. Build entries retain
`build() int32`.

`main` accepts zero parameters, arguments alone, environment alone, or arguments
followed by environment. Parameter names are arbitrary; types determine their
role. Type-returning `comp fn` calls can appear directly in type positions;
aliases are optional. The computed type can also select its enum variants:

```fib
fn main(args: std::process::Arguments, env: std::process::Environment) std::result::Result(int32, void) {
    switch (std::process::getenv(env, "HOME")) {
        when .Some(home) { return std::result::Result(int32, void).Ok(0); }
        when .None { return std::result::Result(int32, void).Err; }
    };
}
```

`Arguments` is `[]const [:0]uint8` and `Environment` is
`std::map::Map(const []uint8, const [:0]uint8)`. Argument strings and environment
values are NUL-terminated views (`.len` excludes the NUL) of process-lifetime host
bytes, so they pass to `std::os` path functions through `slice_data` without
copying, and convert implicitly to `const []uint8` for byte-string helpers:

```fib
fn main(args: std::process::Arguments) int32 {
    if args.len < 2 { return 2; };
    switch (std::os::open(std::memory::slice_data(args[1]), std::os::O_RDONLY, 0)) {
        when .Ok(fd) { std::os::close(fd); return 0; }
        when .Err(error) { return 1; }
    };
}
```

Arguments include the executable name at index zero when provided by the host.
Empty arguments are preserved. Strings need not contain valid UTF-8. Environment
entries split at the first `=`; keys are the preceding bytes (not terminated),
empty values are preserved, malformed entries are ignored, and the last duplicate
key wins. `getenv(env, name)` returns `Option(const [:0]uint8)`.

`std::process::abort(message)` writes the message and a newline to stderr and
exits with status 134 without running pending defers. Startup allocation failure
aborts with a diagnostic; `std::result::expect` and `std::option::expect` also
abort.

`std::map::Map(K, V)` is generic over key and value types and uses linear lookup.
`init(K, V, equal)` creates an empty map with an equality function of type
`fn(K, K) bool`. Keys match according to that function; callers should supply an
equivalence relation. `std::string::eq` supplies bytewise, case-sensitive
equality for `const []uint8` keys and is used by the environment map.
`insert(K, V, map&, key, value)` inserts or replaces and returns
`std::memory::Status` (`Result(void, AllocError)`); on failure the map is
unchanged. Storage comes from the heap allocator. `get(K, V, map, key)` returns
`Option(V)`; an absent key (`None`) differs from a present empty value.
`map.len` counts distinct keys.
`deinit(K, V, map&)` frees owned arrays and resets the map for reuse, preserving
its equality function. Key and value copies follow ordinary Fib value semantics;
pointer/slice backing storage remains borrowed and must outlive map use.
Map copies share storage; keep one owner for mutation and cleanup. The environment
map's ownership passes to `main`; it may be mutated and deinitialized, or left for
process exit to reclaim. Argument descriptor storage is freed after `main` returns.
See [process entry sample](../samples/process_entry.fib).

```fib
fn equal(a: int32, b: int32) bool { return a == b; }
fn main() int32 {
    values := std::map::init(int32, int64, equal);
    defer std::map::deinit(int32, int64, values&);
    switch (std::map::insert(int32, int64, values&, 7, 42)) {
        when .Ok {}
        when .Err { return 2; }
    };
    switch (std::map::get(int32, int64, values, 7)) {
        when .Some(value) { return value as int32; }
        when .None { return 1; }
    };
}
```

Computed type annotations such as `Map(K, V)` and `Option(V)` are accepted
in signatures and storage declarations. They share the existing bounded type
evaluator and specialization cache. See [generic map sample](../samples/generic_map.fib).

The driver generates a source wrapper, analyzes it with the application, and
lowers it through verified SSA and the ordinary C/Fib ABI adapters. Startup uses
the C runtime's argc/argv/envp boundary and source std allocation/parsing.

## Names and prelude

Use `std::memory::slice_data`, `std::os::write`, and
`std::result::Result`. Short names are ordinary aliases:

```fib
comp os := std::os;
comp memory := std::memory;
comp Result := std::result::Result;
```

`std` is protected as a dependency root. `root` denotes the current package,
including from within std. Each package resolves dependency names in its own
environment. Package/file segments normalize hyphens to underscores and use NFC,
full default Unicode case folding, then NFC; declaration names remain case-sensitive.
See the [canonical linkage contract](roadmap/00-cross-module-linkage.md) for table versions
and persistent installed-package identities.

`std::prelude` selects implicit aliases through its public declarations. It is
currently empty; std names require qualification, e.g. `std::string::eq`.
Selected exports preserve canonical identities and obey ordinary
alias/lexical rules, including errors for collisions with explicit file bindings.
The compiler's primitive prelude remains protected independently.

Memory intrinsics now live in `std::memory`; `memory::` requires an explicit
alias. `unchecked_deref` skips its null check, while `slice_from_parts` and
`slice_data` preserve the element type and readonly qualifier. Neither allocation
validity nor view lifetime is guaranteed by these operations.

## Formatting and output

`std::formatting` owns tagged arguments, template validation, integer conversion
and the shared allocation-free renderer. Its base byte-string API is:

```fib
std::formatting::format(destination: []uint8, template: const []uint8, values: const []Arg)
```

It returns `FormatResult = Result(usize, Error)`, with the byte count on success.
The formatted string is `destination[..count]`; no terminating NUL is appended,
and unused destination bytes are unchanged. Syntax, argument count and capacity
are checked before modifying destination, including on failure. Errors are
`InvalidFormat`, `ArgumentCount` and `BufferTooSmall`. Template and `Arg.Bytes`
storage must not overlap destination and must remain unchanged during the call.
Byte strings need not be UTF-8 and may contain NUL bytes. Empty output accepts an
empty, including null-backed, destination.

```fib
comp formatting := std::formatting;
fn main() int32 {
    buffer: [8]uint8 = [0, 0, 0, 0, 0, 0, 0, 0];
    values: [1]formatting::Arg = [formatting::Arg.Unsigned(42)];
    switch (formatting::format(buffer, "id={}", values)) {
        when .Ok(count) {
            switch (std::io::print(buffer[..count])) {
                when .Ok { return 0; }
                when .Err { return 1; }
            };
        }
        when .Err { return 1; }
    };
}
```

`format_to(T, context: *T, writer: fn(*T, const []uint8) FormatResult,
template: const []uint8, values: const []Arg)` renders to a custom sink. The
writer must synchronously consume each complete borrowed chunk and return its
length or an error; it must not retain the slice. Syntax/count errors never call
the writer, while writer errors can leave partial output. `Error.Io(errno)` is
available for I/O sinks. `format` uses the same renderer with a capacity-checking
buffer sink; it neither allocates nor performs I/O.

`std::io::print(bytes: const []uint8)` writes literal bytes to stdout.
`printf(format: const []uint8, values: const []Arg)` formats to stdout;
`fprintf(fd: int32, format, values)` selects a descriptor and
`writef(writer: Writer, format, values)` targets any `Writer`. All return
`PrintResult = Result(usize, PrintError)`, with the number of bytes written on
success; I/O failures are `PrintError.Io(errno)`. `io::Arg`, `io::PrintError` and
`io::PrintResult` are aliases of `formatting::Arg`, `formatting::Error` and
`formatting::FormatResult`. Exhaustive switches on `PrintError` must include
`BufferTooSmall`, though the I/O sinks do not produce it.
`Arg` variants are `Bytes: const []uint8`,
`Signed: int64`, `Unsigned: uint64` and `Boolean: bool`. Arguments are passed in
an explicitly typed array, which implicitly converts to the expected slice type;
explicit slicing also remains valid, as in the [formatting sample](../samples/formatting.fib).

Each `{}` consumes one argument and uses its default representation: raw bytes,
decimal integers or `true`/`false`. `{{` and `}}` emit literal braces. Syntax and
argument counts are validated before output; errors are `InvalidFormat` or
`ArgumentCount`. All argument variants support `{}`, so there is no argument-type
error variant. Width, precision, type specifiers and floating-point formatting
are not implemented. Integer formatting includes the full int64/uint64 range.

Output goes through `io::write_all` (see [streams](#input-and-output-streams)),
so interrupted and short writes are retried. I/O errors can leave partial output.
`writef` delegates to `formatting::format_to` with a `Writer` adapter. Formatting
writes segments directly, uses stack storage for integers and performs no
allocation. Calls are unbuffered and not atomic across segments.

## Randomness

`std::random` provides OS-backed randomness through `std::os::getrandom`
(the Linux syscall with flags zero, or `getentropy` on macOS), waiting for
kernel RNG initialization. There is no predictable
fallback, userspace seed, shared generator state or allocation.

- `fill(buffer: []uint8) Status` fills the complete buffer through
  `io::read_full`, retrying EINTR and short reads. Empty slices succeed without a
  syscall. Errors may leave partial contents; zero progress reports EIO.
- `random_uint64() Uint64Result` returns a uniformly random full-width `uint64`.
- `below(upper: uint64) Uint64Result` samples uniformly from `[0, upper)` using
  rejection sampling rather than biased modulo reduction. Zero returns
  `Error.InvalidBound`; one returns zero without requesting entropy.

`Status` aliases `Result(void, Error)` and `Uint64Result` aliases
`Result(uint64, Error)`. `Error.Io(errno)` carries a `std::os::Errno`,
including ENOSYS when the syscall is unavailable. Other targets have the same
explicit syscall-emission restriction as `std::os`. No deterministic PRNG or
floating-point sampling API is provided.

```fib
fn main() std::result::Result(int32, std::random::Error) {
    switch (std::random::below(6)) {
        when .Ok(value) {
            return std::result::Result(int32, std::random::Error).Ok((value + 1) as int32);
        }
        when .Err(error) {
            return std::result::Result(int32, std::random::Error).Err(error);
        }
    };
}
```

## Results and options

`std::result::Result(T, E)` computes a nominal enum with variants
`Ok: T` and `Err: E`; `std::option::Option(T)` has `Some: T` and a bare `None`.
Repeated applications to identical canonical type arguments produce the same
type. The error type is independent of the operating system.

```fib
comp R := std::result::Result(int32, bool);
fn answer() R { return R.Ok(42); }

comp Status := std::result::Result(void, uint8);
fn success() Status { return Status.Ok; }
```

Switch bindings are the payloads directly, with no `.value` or `.error` wrapper
field. Unit successes (`Result(void, E)`) are bare variants and optionally bind a
void value.

Helpers are ordinary generic functions; pass the payload types first:

| `std::result` | `std::option` | Behavior |
|---|---|---|
| `is_ok(T, E, r)`, `is_err(T, E, r)` | `is_some(T, o)`, `is_none(T, o)` | test the variant |
| `unwrap_or(T, E, r, fallback)` | `unwrap_or(T, o, fallback)` | payload or fallback |
| `expect(T, E, r, message)` | `expect(T, o, message)` | payload, or `process::abort(message)` |
| `map(T, U, E, r, f)` | `map(T, U, o, f)` | transform the payload |
| `map_err(T, E, F, r, f)` | — | transform the error |
| `ok(T, E, r)`, `err(T, E, r)` | `ok_or(T, E, o, error)` | convert between Result and Option |

```fib
comp memory := std::memory;
fn main() int32 {
    words := std::result::expect([]uint64, memory::AllocError, memory::alloc(uint64, 4), "out of memory");
    defer memory::free(uint64, words);
    words[0] = 42;
    return words[0] as int32;
}
```

There is no `?` propagation operator; propagate with a `switch` that returns the
`Err` payload.

## Operating-system boundary

This section describes the Linux x86-64 implementation; the macOS one shares its
API (see [Targets](#targets)). `std::syscall` names every Linux
x86-64 syscall number (0–334 and 424–469) as an upper-case `uint64` constant,
e.g. `std::os::syscall1(std::syscall::CLOSE, fd)`; numbers come from the kernel's
`syscall_64.tbl` (as mirrored by the libc crate).

`std::os` provides typed wrappers for the common POSIX/Unix calls:

- descriptors and I/O: `read`, `write`, `pread`, `pwrite`, `readv`, `writev`,
  `open`, `openat`, `close`, `lseek`, `dup`, `dup2`, `dup3`, `pipe`, `pipe2`,
  `fcntl`, `ioctl`, `fsync`, `fdatasync`, `sync`, `truncate`, `ftruncate`,
  `flock`, `poll`;
- files and directories: `stat`, `lstat`, `fstat`, `fstatat`, `access`,
  `faccessat`, `chmod`, `fchmod`, `chown`, `fchown`, `lchown`, `umask`, `mkdir`,
  `mkdirat`, `rmdir`, `unlink`, `unlinkat`, `rename`, `renameat`, `link`,
  `symlink`, `readlink`, `getcwd`, `chdir`, `fchdir`, `getdents64`;
- memory: `mmap`, `munmap`, `mprotect`, `msync`, `madvise`, `sbrk`;
- processes: `getpid`, `getppid`, `gettid`, `getuid`, `geteuid`, `getgid`,
  `getegid`, `setuid`, `setgid`, `getpgid`, `setpgid`, `getsid`, `setsid`,
  `fork`, `execve`, `exit` (exit_group), `waitpid`, `kill`, `sched_yield`,
  `alarm`, `pause`, `uname`, `getrlimit`, `setrlimit`;
- time and randomness: `clock_gettime`, `clock_getres`, `nanosleep`, `getrandom`;
- sockets: `socket`, `socketpair`, `bind`, `connect`, `listen`, `accept`,
  `accept4`, `shutdown`, `send`, `recv`, `sendto`, `recvfrom`, `setsockopt`,
  `getsockopt`, `getsockname`, `getpeername`.

The module also defines the matching kernel structures (`Stat`, `Timespec`,
`Utsname`, `Rlimit`, `Iovec`, `PollFd`, `FdPair`, `WaitStatus`) and flag, signal
and errno constants (`O_*`, `AT_*`, `SEEK_*`, `F_*`, `S_IF*`, `SIG*`, `CLOCK_*`,
`POLL*`, `RLIMIT_*`, `AF_*`, `SOCK_*`, `E*`, …). Flag constants are typed
(`int32` for open/mmap/signal/socket flags, `uint32` for `S_IF*`, `int16` for
`POLL*`), and `stdin`/`stdout`/`stderr` are `int32` descriptors. Kernel-filled
structures are returned by value (`stat(path)` returns `StatResult`); socket addresses
are untyped `*void`/`const *void` pointers plus a byte length. `exit` returns
`never`: a call ends the caller's path (no trailing `return` is needed) and
pending defers do not run. Calls without a typed wrapper
use the raw forms with `std::syscall` numbers.

Named wrappers return `Result` values whose `Err` payload is a
`std::os::Errno`: the positive kernel error number. `Errno` aliases `int32`, so
errors compare directly with the `E*` constants (`error == std::os::ENOENT`), and
`errno_name(error)` returns its symbolic name (`"ENOENT"`, or `"EUNKNOWN"`).
Aliases name the common shapes: `Status = Result(void, Errno)`,
`CountResult = Result(usize, Errno)`, `IntResult = Result(int32, Errno)`,
`OffsetResult`, `MappingResult`, `StatResult`, `TimeResult`, and so on.
Identity queries that cannot fail (`getpid`, `getppid`, `gettid`, `getuid`, …)
return their value directly.

The I/O wrappers make one syscall: short reads/writes, EOF and EINTR are returned
to the caller without retrying; use `std::io::read_full`/`write_all` with
`fd_reader`/`fd_writer` to retry. Path parameters are `const [*:0]uint8`: pass
`c"/dev/null"`, or `std::memory::slice_data(view)` for a terminated slice such as
a process argument. `std::memory::span(pointer)` converts the other way. Flags,
mode and mmap offset follow Linux's contracts; no implicit allocation or path
conversion occurs.

`read` takes a writable byte slice; `write` takes a readonly byte slice. Mmap
returns a byte pointer on success. Callers maintain buffer/mapping lifetimes and
mapping permissions and explicitly call close/munmap, optionally through `defer`.
There are no owning handles or automatic destruction in this milestone.

Raw `syscall0` through `syscall6` take a `uint64` number and the corresponding
number of `uint64` words or typed pointer arguments, returning the raw `int64`
kernel result. Known integer literals receive uint64 context; other numeric
values require explicit casts. Pointer/register conversions happen internally.
The typed `pointer_from_word(T, uint64)` and `pointer_to_word(pointer)` helpers
are package-restricted; they do not expose public pointer/integer conversions.
The package-restricted `terminated_from_parts(pointer: *uint8, length) [:0]uint8`
(readonly pointers give readonly slices) trusts the caller that `pointer[length]`
is zero; std uses it only after scanning for the terminator.

`sbrk(increment: int64)` returns `MappingResult`: the previous break on success
or `Err(ENOMEM)` on failure. Zero queries the break; negative increments
shrink it. It queries Linux on every call, checks arithmetic before changing the
break, and compares the raw `brk` result to the requested address. Linux returns
the unchanged break on failure, rather than a negative errno. See
[brk(2)](https://man7.org/linux/man-pages/man2/brk.2.html).

## Allocation

`std::memory::memset(destination: []uint8, value: uint8)` fills every destination
byte. `std::memory::memcpy(destination: []uint8, source: const []uint8)` copies
all source bytes into the destination prefix, leaving the tail unchanged. Both
return void. Pass subslices to select a byte count. Copying requires nonoverlapping
regions and traps before writing if the destination is shorter than the source.
Empty operations do not access storage, including for null-backed empty slices.
These ordinary Fib functions use checked byte loops; they do not convert element
types. See [memory sample](../samples/memory.fib).

### Allocators

Every allocator is a `std::memory::Allocator` value:

```fib
pub comp Allocator := struct {
    context: *void,
    alloc: fn(*void, usize, usize) AllocResult, // size, alignment
    free: fn(*void, *uint8) Status,
};
```

`AllocResult` is `Result(*uint8, AllocError)` and `Status` is
`Result(void, AllocError)`. `alloc` returns uninitialized storage of a nonzero
size; alignment must be a positive power of two that the implementation supports.
`free` accepts only an original live pointer from the same allocator, or null.
`AllocError` variants are `NotEnoughMemory` (zero size, arithmetic overflow or
exhaustion), `InvalidAlignment`, `MapFailed`, `UnmapFailed` and `InvalidFree`.
Alignment validation precedes size validation.

Two implementations are provided. Both ignore their null context, so allocator
values can be copied freely:

- `heap_allocator()`: a single-threaded first-fit free list. Fresh storage comes
  from the package-private `os::heap_grow`: the program break on Linux, private
  mappings on macOS. It supports alignment up to 16, splits oversized free
  blocks, coalesces physically adjacent free blocks and keeps storage for reuse
  rather than returning it to the OS. Freeing an unknown or already-free pointer returns `InvalidFree` and leaves
  the heap unchanged. It does not coordinate with libc's allocator or concurrent
  break manipulation; do not shrink the break over live storage. It tolerates
  alignment gaps from intervening break growth.
- `page_allocator()`: one anonymous private read/write mapping per allocation,
  with private metadata before each block so `free` needs no size. Any
  power-of-two alignment is supported, including over-page values. Kernel ENOMEM
  reports `NotEnoughMemory`, other mapping failures `MapFailed`, and release
  failures `UnmapFailed` (metadata stays intact so release can be retried). The
  header is trusted: unknown pointers and double frees are undefined.

The typed helpers work with any allocator:

- `create(T, allocator, count) Result([]T, AllocError)` allocates `count`
  uninitialized elements with 16-byte alignment (enough for the supported Linux
  x86-64 types), checking the size arithmetic. Zero counts return an empty slice
  without allocating; zero-sized element types still receive a live allocation.
- `destroy(T, allocator, items) Status` releases a slice returned by `create`
  with the same allocator. Empty slices are ignored.
- `alloc(T, count)` and `free(T, items)` are `create`/`destroy` with
  `heap_allocator()`.

Every view expires when its storage is freed; no lifetime checking is provided.
See [allocator sample](../samples/allocator.fib).

```fib
fn main() int32 {
    page := std::memory::page_allocator();
    switch (std::memory::create(uint8, page, 100)) {
        when .Err { return 1; }
        when .Ok(bytes) {
            bytes[0] = 42;
            switch (std::memory::destroy(uint8, page, bytes)) {
                when .Ok { return 0; }
                when .Err { return 1; }
            };
        }
    };
}
```

`span(pointer: const [*:0]uint8) const [:0]uint8` views a C string as a
terminated slice excluding the NUL, scanning for the terminator without copying.

### Vectors

`std::vector::Vec(T)` is a growable array whose storage comes from an explicit
`Allocator`. Operations are type-owned members called with an explicit receiver:

| Member | Result |
|---|---|
| `init(allocator)` | an empty `Vec` |
| `push(v&, element)` | `memory::Status`; the element is appended only on `Ok` |
| `reserve(v&, additional)` | `memory::Status`; ensures room for `additional` more elements |
| `pop(v&)` | `Option(T)`: the last element, or `None` when empty |
| `get(v&, index)` | `Option(T)`: the element, or `None` when out of bounds |
| `items(v&)` | `[]T` view of the elements; writes update the Vec |
| `capacity(v&)`, `v.len` | spare-inclusive capacity and element count |
| `clear(v&)` | drop elements, keep capacity |
| `deinit(v&)` | `memory::Status`; release storage and reset |

```fib
comp Numbers := std::vector::Vec(int32);
fn main() int32 {
    numbers := Numbers.init(std::memory::heap_allocator());
    defer Numbers.deinit(numbers&);
    switch (Numbers.push(numbers&, 42)) {
        when .Ok {}
        when .Err { return 1; }
    };
    return Numbers.items(numbers&)[0];
}
```

Capacity grows geometrically (minimum 4). Growth moves elements, so `items()`
views expire after `push`, `reserve` or `deinit`. Copies of a `Vec` share storage:
keep one owner and call `deinit` once.

### Byte-string helpers

Ordinary byte strings are `const []uint8`; NUL-terminated ones are
`const [:0]uint8` and convert implicitly, so every helper accepts both. Lengths
count bytes, with no encoding assumption.

- `std::string::eq(a, b) bool` compares contents bytewise, including empty
  slices; it doubles as a `std::map` equality function.
- `std::string::index_of(value, byte) Option(usize)` finds the first occurrence.
- `std::string::concat(destination: []uint8, a, b)` returns
  `ConcatResult` (`Result([]uint8, std::string::Error)`). Capacity checks precede
  all writes and avoid length overflow; `BufferTooSmall` leaves destination
  unchanged. Success returns its written prefix, preserves the tail and adds no
  NUL. Sources must not overlap destination. No allocation occurs; the result
  borrows destination without extending its lifetime.

Build files may load runtime modules containing unused global state. The narrow
build-file interpreter still rejects global accesses and allocator calls.

Emission uses the [Linux register ABI](https://github.com/torvalds/linux/blob/master/arch/x86/entry/entry_64.S)
and side-effecting LLVM inline assembly with register, flags and memory clobbers.
No native shim or libc syscall wrapper is linked. The ordinary executable entry
and host-linker startup contract remain in use. Other targets reject syscall
emission explicitly; build-file interpretation rejects syscall execution.

Conditional compilation, portable flags/errors, higher-level I/O, advanced formatting,
collections, concurrent allocation and compiler-enforced resource ownership remain future work.
