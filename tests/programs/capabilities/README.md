# Capability regression fixture

These sources preserve the former `example-project` capability demonstration so
compiler regressions do not depend on the evolving user-facing example. Both
`src/driver/test.rs` and `tests/executable.rs` copy all three Fib files into a
temporary directory and compile them through `build.fib`. Build API arguments use
explicit string slices (`[..]`).

The executable returns zero when every check passes. Nonzero exit codes identify
the failed section:

1. Integer widths, wrapping and casts
2. Floating-point arithmetic, conversions and IEEE values
3. Bitwise operations and population count
4. Nested loops, break and continue
5. Recursion
6. Short-circuit evaluation
7. Structs, storage and pointers
8. Enums, payload copies and exhaustive switches
9. Generic specialization, fixed arrays and multi-bindings
