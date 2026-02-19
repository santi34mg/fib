# Feature Planning Document

This document discusses design decisions for upcoming fib language features.
Each section presents the problem, explores different approaches, and recommends a direction that aligns with fib's core principles: **developer control**, **zero-cost abstractions**, and **predictable performance**.

---

## Table of Contents

1. [I/O](#7-io)
2. [Threading](#8-threading)
3. [Libraries, Packaging, and Remote Code](#9-libraries-packaging-and-remote-code)

## 7. I/O

### Problem Statement

How should I/O be handled? Is it entirely standard library, or are language features needed?

### Approaches

| Approach                            | Description                         | Pros                     | Cons                                |
| ----------------------------------- | ----------------------------------- | ------------------------ | ----------------------------------- |
| **A: Pure standard library**        | All I/O via library functions       | Minimal core             | May lack optimization opportunities |
| **B: Effect-based I/O**             | I/O as effects, handlers in library | Pure functions, testable | Complex                             |
| **C: Capability-based**             | I/O requires capability tokens      | Secure, explicit         | Verbose                             |
| **D: Traditional syscall wrappers** | Thin wrappers over OS               | Fast, predictable        | Platform-specific                   |

### Recommendation: Standard Library with Language Support for Errors (A+D)

#### Core Principle

I/O is inherently side-effecting and platform-specific. Keep it in the standard library but provide good patterns.

#### Error Handling Integration

```fib
// I/O functions return sum types
// Example error handling pattern:
// let result = match read_file("data.txt") {
//   'Ok data -> data
//   'Err e -> { print("Error: " + e:get_error_message()); return; }
// };
```

#### With Statement for Resources

```fib
// RAII-style resource management
with file = io:open("data.txt", 'Read)? {
    let content string = io:read_all(file).ok();
    process(content);
}  // file automatically closed
```

#### Async I/O

Async I/O uses the async system (no generics)

## 8. Threading

### Problem Statement

How does threading work? How does it interact with async and I/O?

### Approaches

| Approach                      | Description                       | Pros                    | Cons                         |
| ----------------------------- | --------------------------------- | ----------------------- | ---------------------------- |
| **A: OS threads via library** | Thin wrappers over pthreads/Win32 | Predictable, no runtime | Manual synchronization       |
| **B: Thread pools**           | Fixed pool of worker threads      | Efficient resource use  | Complexity                   |
| **C: Actor model**            | Message-passing between actors    | No shared state         | Overhead, different paradigm |
| **D: Structured concurrency** | Scoped thread lifetimes           | Safe, leak-free         | Restrictive                  |

### Recommendation: OS Threads + Structured Concurrency (A+D)

#### Core Threading (Standard Library)

```fib
module thread;

// Example thread usage (no generics):
// let t = thread:spawn(function() { ... });
// thread:join(t);
```

#### Structured Concurrency

// Structured concurrency patterns can be implemented with code generation or macros.

#### Synchronization Primitives

// Synchronization primitives should be provided as concrete types or via code generation, not generics.

#### Relationship with Async

// Async/threading composition patterns should use code generation or macros, not generics.

#### Debugging Support

```fib
// Thread naming for debugging
let t Thread = thread:spawn_named("worker-1", worker_func);

// Debug hints
@debug_thread_id
function log(msg string) {
    print("[Thread " + thread:current_id() + "] " + msg);
}

// Data race detection (debug builds)
// Compiler inserts checks when @synchronized data accessed
```

---

## 9. Libraries, Packaging, and Remote Code

### Problem Statement

How do developers use external code? Static/dynamic linking? Package management?

### Approaches

| Approach                             | Description                               | Pros                     | Cons                        |
| ------------------------------------ | ----------------------------------------- | ------------------------ | --------------------------- |
| **A: Vendoring only**                | Copy dependencies into project            | Simple, reproducible     | Manual updates, large repos |
| **B: Central package registry**      | npm/crates.io style                       | Easy discovery, versions | Single point of failure     |
| **C: Decentralized (Git URLs)**      | Go-style direct imports                   | No registry needed       | Version management harder   |
| **D: Hybrid (registry + vendoring)** | Registry for discovery, vendor for builds | Best of both             | More complexity             |

### Recommendation: Hybrid Approach (D)

#### Project Structure

```
my_project/
├── fib.toml              # Project manifest
├── fib.lock              # Lock file (exact versions)
├── src/
│   ├── main.fib
│   └── lib/
│       └── utils.fib
├── vendor/               # Optional: vendored dependencies
│   └── http/
│       └── ...
└── target/
    └── ...
```

#### Manifest File (fib.toml)

```toml
[package]
name = "my_project"
version = "1.0.0"
authors = ["Developer <dev@example.com>"]

[dependencies]
# From registry
http = "2.1.0"
json = "^1.0"        # semver compatible

# From git
my_lib = { git = "https://github.com/user/my_lib", tag = "v1.0.0" }

# Local path
utils = { path = "../shared/utils" }

[dev-dependencies]
test_helpers = "1.0"

[build]
# Linking preferences
link_type = "static"  # or "dynamic", "prefer-static"
```

#### Import Syntax

```fib
// Import from dependency
import http;
import json:parse;
import json:{ parse, stringify };

// Import from local module
import my_project:utils;

// Qualified vs unqualified
import http;           // use as http:get()
import http:*;         // use as get() (glob import, discouraged)
import http:get as fetch;  // rename
```

#### Package Manager Commands

```bash
# Initialize new project
fib init my_project

# Add dependency
fib add http
fib add http@2.1.0
fib add https://github.com/user/lib --git

# Update dependencies
fib update
fib update http

# Vendor dependencies (copy to vendor/)
fib vendor
fib vendor --all

# Build
fib build
fib build --release

# Run
fib run

# Test
fib test
```

#### Linking

```fib
// In fib.toml
[build]
link_type = "static"  // Default: statically link all dependencies

// Or per-dependency
[dependencies.openssl]
version = "1.1"
link = "dynamic"  // Use system OpenSSL

// FFI for C libraries
[dependencies.sqlite]
version = "3.0"
native = true  // Has C code
link = "static"
```

#### Module Resolution

1. Check local `src/` directory
2. Check `vendor/` directory
3. Check downloaded packages in `~/.fib/packages/`
4. If not found, download from registry/git

#### Vendoring Strategy

```bash
# Vendor all dependencies for reproducible builds
fib vendor --all

# Vendor specific dependency
fib vendor http

# Build using only vendored code (offline)
fib build --frozen
```

#### Security

```toml
# fib.toml
[security]
# Require checksum verification
verify_checksums = true

# Allow/deny specific registries
allowed_registries = ["https://registry.fib-lang.org"]

# Audit for known vulnerabilities
audit = true
```

#### Publishing

```bash
# Publish to registry
fib publish

# Publish to specific registry
fib publish --registry https://private.company.com
```

#### Why This Design?

1. **Reproducibility**: Lock files + vendoring ensure builds are reproducible
2. **Flexibility**: Support registry, git, and local dependencies
3. **Security**: Checksums, auditing, registry control
4. **Simplicity**: Single manifest file, familiar to Cargo/npm users
5. **Offline capable**: Vendoring allows fully offline builds

---

## Summary Matrix

| Feature        | Built-in                      | Standard Library     | Recommendation          |
| -------------- | ----------------------------- | -------------------- | ----------------------- |
| Pointers       | `ptr T`, `addressof`, `deref` | —                    | Keywords for safety     |
| Arenas         | `new(alloc)` syntax           | `Arena`, `Allocator` | Arena-first design      |
| Fixed Arrays   | `[N]T`                        | —                    | Value type, stack       |
| Slices         | `slice T`, `[a..b]`           | —                    | Fat pointer view        |
| Dynamic Arrays | —                             | `Vec T`              | Library type            |
| Maps           | Literal syntax `{ k -> v }`   | `Map K V`            | Hybrid                  |
| Async          | `async`, `await`, `Future`    | Executors            | Stackless coroutines    |
| Generators     | `generator`, `yield`          | —                    | For iteration           |
| I/O            | `?` operator, `with`          | All I/O functions    | Library-based           |
| Threading      | —                             | `thread`, `sync`     | OS threads + structured |
| Packages       | `import`                      | —                    | Manifest + registry     |

---

## Next Steps

1. **Prototype** each feature in isolation
2. **Write tests** that exercise edge cases
3. **Document** each feature thoroughly
4. **Gather feedback** from potential users
5. **Iterate** based on real-world usage

---

## Open Questions

1. Should `ptr` allow arithmetic, or require explicit functions?
2. Should slices be mutable by default, or have `mut_slice T`?
3. How do generics interact with contracts for `Map K V`?
4. Should `async` functions require explicit annotation, or be inferred?
5. What's the default behavior when a thread panics?
6. Should the package registry be centralized or federated?
