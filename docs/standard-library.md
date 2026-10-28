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
values, array lengths or global initializers. Compiler-declaration revision 3
records this compiler contract; reinstall sources alongside the compiler.

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
environment. Package/file segments fold ASCII case and normalize hyphens to
underscores; declaration names remain case-sensitive. Unicode normalization
remains unspecified; non-ASCII code points are currently preserved.

`std::prelude` selects implicit aliases through its public declarations. It is
initially empty. Selected exports preserve canonical identities and obey ordinary
alias/lexical rules, including errors for collisions with explicit file bindings.
The compiler's primitive prelude remains protected independently.

Memory intrinsics now live in `std::memory`; `memory::` requires an explicit
alias. `unchecked_deref` skips its null check, while `slice_from_parts` and
`slice_data` preserve the element type and readonly qualifier. Neither allocation
validity nor view lifetime is guaranteed by these operations.

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

`std::memory::alloc(T, n: uint32)` returns a writable `T[]` of length `n`.
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

Conditional compilation, portable flags/errors, higher-level I/O, formatting,
collections, concurrent allocation and compiler-enforced resource ownership remain future work.
