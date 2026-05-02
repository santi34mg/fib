# Imports and Modules

Modules are organized in directories under an include path (e.g. `std/`). Path segments are separated by `::`.

## Basic import

```fib
import std::libc
import std::core::error
```

After import, names are referenced via the qualified path:

```fib
libc::printf("Hello, World!\n")
libc::malloc(16 as uint)
```

## Selective import

Import specific names from a module with `::{ ... }`:

```fib
import std::core::error::{Error}
import std::fs::path::{Path}
```

The selected names can then be used unqualified:

```fib
ret Error { message: "stream is empty", code: 1 }
```

## Aliasing

Rename an import with `as`:

```fib
import std::libc as c

c::printf("hi\n")
```

## Include path

When invoking the compiler, pass `-I=<dir>` to add a directory to the module search path. The `std` directory shipped with the project is included this way:

```
cargo run -- samples/hello_world.fib -I=std
```
