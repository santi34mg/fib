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
values, array lengths or global initializers. Compiler-declaration revision 4
records prefix container syntax, sentinel byte types and the terminated `openat`
path contract; reinstall sources alongside the compiler.

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

## Process entry and maps

On Linux x86-64, `main` may return `int32`, `void`, or an instance of
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
    switch (std::map::get(std::process::Bytes, std::process::Bytes, env, "HOME"[..])) {
        when .Ok(home) { return std::result::Result(int32, void).Ok(0); }
        when .Err { return std::result::Result(int32, void).Err; }
    };
}
```

`std::process::Bytes` is `const []uint8`; `Arguments` is `[]Bytes` and
`Environment` aliases `std::map::Map(Bytes, Bytes)`. Arguments include the executable name at
index zero when provided by the host. Empty arguments are preserved. Argument
and environment strings borrow process-lifetime bytes, exclude terminating NULs,
and need not contain valid UTF-8. Environment entries split at the first `=`;
empty values are preserved, malformed entries are ignored, and the last duplicate
key wins. Startup allocation failure exits with status 1.

`std::map::Map(K, V)` is generic over key and value types and uses linear lookup.
`init(K, V, equal)` creates an empty map with an equality function of type
`fn(K, K) bool`. Keys match according to that function; callers should supply an
equivalence relation. `bytes_equal` supplies bytewise, case-sensitive equality for
`const []uint8` keys and is used by the environment map.
`insert(K, V, map&, key, value)` inserts or replaces and returns false on allocation
failure without changing the map. `get(K, V, map, key)` returns `Result(V, void)`;
an absent key differs from a present empty value. `map.len` counts distinct keys.
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
    std::map::insert(int32, int64, values&, 7, 42);
    switch (std::map::get(int32, int64, values, 7)) {
        when .Ok(value) { return value as int32; }
        when .Err { return 1; }
    };
}
```

Computed type annotations such as `Map(K, V)` and `Result(V, void)` are accepted
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
currently empty; string helpers require qualification through `std::string`.
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
`fprintf(fd: int32, format: const []uint8, values: const []Arg)` selects a
descriptor. All return `PrintResult = Result(usize, PrintError)`, with the number
of bytes written on success. `io::Arg`, `io::PrintError` and `io::PrintResult`
remain aliases of `formatting::Arg`, `formatting::Error` and
`formatting::FormatResult`. Exhaustive switches on the former `PrintError` must
now include `BufferTooSmall`, though the built-in I/O sink does not produce it.
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

`write_all(fd: int32, bytes: const []uint8)` retries interrupted and short writes;
empty writes make no syscall. Other errors return `PrintError.Io(errno)`, with
positive errno; a zero-progress write reports EIO (5). I/O errors can leave
partial output. `printf` and `fprintf` delegate to `formatting::format_to` with
an I/O writer adapter. Formatting writes segments directly, uses stack storage
for integers and performs no allocation. Calls are not atomic across segments.

## Randomness

`std::random` provides OS-backed randomness on Linux x86-64 using `getrandom`
with flags zero, waiting for kernel RNG initialization. There is no predictable
fallback, userspace seed, shared generator state or allocation.

- `fill(buffer: []uint8) Status` fills the complete buffer, retrying EINTR and
  short reads. Empty slices succeed without a syscall. Errors may leave partial
  contents; zero progress reports EIO (5).
- `uint64_value() Uint64Result` returns a uniformly random full-width `uint64`.
- `below(upper: uint64) Uint64Result` samples uniformly from `[0, upper)` using
  rejection sampling rather than biased modulo reduction. Zero returns
  `Error.InvalidBound`; one returns zero without requesting entropy.

`Status` aliases `Result(void, Error)` and `Uint64Result` aliases
`Result(uint64, Error)`. `Error.Io(errno)` carries a positive Linux errno,
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

## Results

`std::result::Result(T, E)` computes a nominal enum with variants
`Ok: T` and `Err: E`. Repeated applications to identical
canonical type arguments produce the same type. The error type is independent
of the operating system.

```fib
comp R := std::result::Result(int32, bool);
fn answer() R { return R.Ok(42); }

comp Status := std::result::Result(void, uint8);
fn success() Status { return Status.Ok; }
```

Computed types are instantiated in `comp` bindings; current type annotations
name those aliases. OS wrappers expose `CountResult = Result(usize, int32)`,
`IntResult = Result(int32, int32)`, and `MappingResult = Result(*uint8, int32)`.
Close and munmap return `std::os::Status`, whose variants are payloadless `Ok`
and `Err: int32`. Switch bindings are the result/error values directly, with no
`.value` or `.error` wrapper field. Unit successes optionally bind a void value.

## Operating-system boundary

`std::os` currently implements Linux x86-64. It provides `read`, `write`, `close`,
`getpid`, `openat`, `mmap`, `munmap`, and `sbrk`. Named wrappers return typed results with
positive errno values. The I/O wrappers make one syscall: short reads/writes, EOF and EINTR
are returned to the caller without retrying. `openat` accepts a live,
NUL-terminated `const *uint8`, for example `c"/dev/null"`. Flags, mode and mmap
offset follow Linux's contracts; no implicit allocation or path conversion occurs.

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

`sbrk(increment: int64)` returns `MappingResult`: the previous break on success
or `Err(12)` (ENOMEM) on failure. Zero queries the break; negative increments
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

### Page allocator

`std::memory::create_page_alloc()` returns a public `Allocator` with a context
pointer and `alloc`/`free` callbacks. It uses Linux x86-64 anonymous private
read/write mappings, independently of the `sbrk` allocator below. Its context is
null and ignored; allocator values can be copied without sharing mutable state.

- `allocator.alloc(allocator.context, size, alignment)` returns
  `Result(*uint8, AllocError)`. Alignment must be a positive power of two; alignments
  larger than the page size are supported. The requested bytes are writable and
  initially zero on this implementation, but the general allocator contract is
  uninitialized storage.
- `allocator.free(allocator.context, pointer as *void)` returns
  `Result(void, AllocError)`. Null succeeds without a syscall. Other pointers must
  be original live allocations from this page allocator, not interior pointers,
  pointers from `alloc(T, n)`, or already-freed pointers. A successful release
  unmaps the storage immediately; all borrowed views expire.

Private metadata before each returned allocation stores the mapping base and
length, so free needs no size parameter. Requested size, metadata and alignment
padding arithmetic are checked before mapping. `AllocError.InvalidAlignment` rejects
zero/non-power-of-two alignment; `NotEnoughMemory` reports zero size, arithmetic
overflow or kernel ENOMEM. Other mapping failures report `MapFailed`; release
failures report `UnmapFailed`, leaving metadata intact so release can be retried.
Alignment validation takes precedence over zero-size validation. The former
placeholder `NotImplemented` error variant has been removed.

```fib
fn main() int32 {
    allocator := std::memory::create_page_alloc();
    switch (allocator.alloc(allocator.context, 100, 16)) {
        when .Err { return 1; }
        when .Ok(pointer) {
            bytes := std::memory::slice_from_parts(pointer, 100);
            bytes[0] = 42;
            switch (allocator.free(allocator.context, pointer as *void)) {
                when .Ok { return 0; }
                when .Err { return 1; }
            };
        }
    };
}
```

### Byte-string helpers

`std::string::concat(destination: []uint8, a: const []uint8, b: const []uint8)`
returns `std::string::ConcatResult` (`Result([]uint8, std::string::Error)`).
Capacity checks precede all writes and avoid length overflow; `BufferTooSmall`
leaves destination unchanged. Success returns its written prefix, preserves the
tail and adds no NUL. Sources must not overlap destination. No allocation occurs;
the result borrows destination without extending its lifetime.
`std::string::str_len(const []uint8) usize` wraps `.len`. Lengths count bytes,
with no encoding assumption. `std::string::str_eq(const []uint8, const []uint8) bool`
compares byte contents, including empty slices.

### Typed heap allocator

`std::memory::alloc(T, n: usize)` returns a writable `[]T` of length `n`.
Zero count, overflow or allocation failure returns an empty slice. Storage is
uninitialized: write elements before reading them. Zero-sized types receive a
live allocation for nonzero counts. Blocks have 16-byte alignment for the
supported Linux x86-64 runtime types, with checked count/size arithmetic.

`std::memory::free(T, ptr: *T)` releases the allocation whose original pointer
is `ptr`. Obtain it with `std::memory::slice_data(slice)`. Null and unknown
pointers are ignored; passing a stale pointer after reuse is invalid. Every view
expires when its allocation is freed. No automatic lifetime checking is provided.

The allocator uses a linked first-fit list, splits oversized free blocks and
coalesces physically adjacent free blocks. It retains storage for reuse rather
than returning it to Linux. It is single-threaded and does not coordinate with
libc's allocator or concurrent break manipulation. Do not shrink the break over
live allocator storage. It tolerates alignment gaps from intervening break growth.
See [allocator sample](../samples/allocator.fib).

Build files may load runtime modules containing unused global state. The narrow
build-file interpreter still rejects global accesses and allocator calls.

Emission uses the [Linux register ABI](https://github.com/torvalds/linux/blob/master/arch/x86/entry/entry_64.S)
and side-effecting LLVM inline assembly with register, flags and memory clobbers.
No native shim or libc syscall wrapper is linked. The ordinary executable entry
and host-linker startup contract remain in use. Other targets reject syscall
emission explicitly; build-file interpretation rejects syscall execution.

Conditional compilation, portable flags/errors, higher-level I/O, advanced formatting,
collections, concurrent allocation and compiler-enforced resource ownership remain future work.
