# Standard-library foundation

`std` is a version-matched source dependency installed with the compiler. Its
files use the same package lookup, visibility and alias rules as application
dependencies. Ordinary Fib functions and compiler intrinsics can share a file
namespace; compiler operations are selected by canonical declaration identity.

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
`Ok { value: T }` and `Err { error: E }`. Repeated applications to identical
canonical type arguments produce the same type. The error type is independent
of the operating system.

```fib
comp R := std::result::Result(int32, bool);
fn answer() R { return R.Ok(42); }
```

Computed types are instantiated in `comp` bindings; current type annotations
name those aliases. OS wrappers expose `CountResult = Result(usize, int32)`,
`IntResult = Result(int32, int32)`, and `MappingResult = Result(*uint8, int32)`.
Close and munmap return `std::os::Status`, whose variants are payloadless `Ok`
and `Err { error: int32 }`.

## Operating-system boundary

`std::os` currently implements Linux x86-64. It provides `read`, `write`, `close`,
`getpid`, `openat`, `mmap`, and `munmap`. Named wrappers return typed results with
positive errno values. They make one syscall: short reads/writes, EOF and EINTR
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
The helper that constructs mmap's pointer result is package-restricted and is
not a public pointer/integer conversion API.

Emission uses the [Linux register ABI](https://github.com/torvalds/linux/blob/master/arch/x86/entry/entry_64.S)
and side-effecting LLVM inline assembly with register, flags and memory clobbers.
No native shim or libc syscall wrapper is linked. The ordinary executable entry
and host-linker startup contract remain in use. Other targets reject syscall
emission explicitly; build-file interpretation rejects syscall execution.

Conditional compilation, portable flags/errors, higher-level I/O, formatting,
collections, allocators and compiler-enforced resource ownership remain future work.
