# Fixed-width scalar semantics

This reference records the implemented [numeric rules](language-specification.md#numeric-and-boolean-behavior).
Fixed-width integer and IEEE arithmetic, direct-literal cast context and explicit
numeric/bool conversions execute through analysis, constant evaluation, verified
SSA and LLVM.

## Types and operation widths

Already typed numeric values require explicit casts to change type, including
assignment, initialization, arguments and returns. Binary operands have matching
types; shift counts may have another integer type. Numeric literals adopt a
numeric context when representable and otherwise default to int32 or float64.
A typed operand supplies the context for an accompanying literal.

Destinations do not change arithmetic involving typed operands:

```fib
fn example(a: uint8) uint16 {
    return (a + 1) as uint16;
}
```

For `a = 255`, this returns zero: addition wraps in uint8 before the cast.
`(a as uint16) + 1` instead produces 256. Literal-only expressions may use their
destination context. In the confirmed design, an `as` destination supplies context
to a direct numeric literal in the same family, including parentheses and unary
minus. Integer literals use integer context; float literals use float context.
`4294967296 as uint64` is valid and `0.1 as float32` rounds directly to float32;
`300 as uint8` and `(-1) as uint8` are literal-range errors. Typed operands,
arithmetic expressions and cross-family casts still resolve their source types
before conversion. Thus `3.9 as int32` truncates to 3 and a typed int32 value 300
casts to uint8 as 44.

Integer magnitudes are parsed without a u64 limit, and decimal floats retain their
spelling until rounding directly to the selected format. The existing literal
spelling grammar is unchanged; exponent notation is not introduced here.

## Integers

Addition, subtraction, multiplication, negation and left shifts retain the low
bits at the operation width. Unsigned right shifts zero-fill; signed right shifts
sign-extend. Signed division truncates toward zero and remainder has the dividend's
sign. MIN / -1 wraps to MIN, and MIN % -1 is zero.

Division or remainder by zero traps. Shift counts must be nonnegative and smaller
than the left width; counts are validated before conversion to that width.

Explicit integer casts narrow by retaining low bits, widen using source
signedness (sign extension or zero extension), and interpret the resulting bits
as the destination type. Equal-width signed/unsigned casts preserve bits.

Out-of-range literal values are errors, including negative integer literals in
unsigned contexts. Arithmetic on valid typed constants wraps just like runtime
arithmetic. Known zero divisors and invalid shift counts receive type diagnostics.

## Floats and numeric casts

Floating arithmetic uses IEEE round-to-nearest, ties-to-even, with gradual
underflow, signed zero, infinity and NaN. No fast-math or contraction is enabled.
NaN compares unequal to every value, including itself; ordering is false for NaN.
NaN payload propagation is unspecified. Floating division by zero follows IEEE
rules. Overflowing literals are diagnosed; arithmetic overflow produces infinity.

Integer-to-float and float-to-float casts round to the destination format;
integer-to-float can lose precision. Float-to-integer casts truncate toward zero,
then check the result's representability. NaN, infinity and unrepresentable
results trap. Thus -0.9 converts to unsigned zero, while -1.0 cannot.
Constant evaluation follows the same rules, diagnosing known trapping operations.

## Booleans and execution boundaries

Booleans support equality, inequality, negation, assignment, calls and returns.
`&&` and `||` evaluate left-to-right and short-circuit. Conditions and logical
operators require bool; there is no implicit numeric truthiness. The confirmed
design permits explicit casts in both directions: numeric zero (including both
floating signed zeros) becomes false, and all other numeric values, including
infinities and NaN, become true; bool becomes numeric 0 or 1. These casts execute in constant evaluation and native lowering. Whole-source validation checks unsupported source forms and known
invalid operations in dead branches.

The native entry remains `main() int32`. Internal helpers use a limited scalar
Fib ABI mapped to LLVM fastcc; definitions and calls share one classification.
This is not an external C interoperability contract or the full production Fib
ABI. Target legalization may use compiler-runtime helpers supplied by host cc.

The build-file interpreter retains its narrow literal byte-string API subset.
Char, runtime globals and runtime indexing/slices remain unsupported by native
lowering. Target-width usize, fixed-array values, concrete generic instances,
structs, enums, typed pointers and stack storage are supported; see [struct/storage semantics](struct-storage-semantics.md) and
[enum semantics](enum-semantics.md).
