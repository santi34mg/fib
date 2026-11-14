// C side of the C-ABI conformance program (tests/programs/c_abi.fib). Covers
// the struct classes that differ between System V x86-64 and AAPCS64:
// register pairs, homogeneous float aggregates, 16-byte alignment, indirect
// aggregates, register exhaustion, sub-word extension and C -> Fib calls.
#include <stdbool.h>
#include <stdint.h>

typedef struct { int32_t a, b; } Pair;          // 8 bytes
typedef struct { int32_t a, b, c; } Triple;     // 12 bytes
typedef struct { float x, y, z; } Vec3;         // float aggregate
typedef struct { double x, y; } Vec2d;          // double aggregate
typedef struct { int64_t a, b, c; } Big;        // 24 bytes, indirect
typedef struct { double d; int32_t i; } Mixed;  // 16 bytes, mixed classes
typedef struct { __int128 v; } Wide;            // 16-byte aligned
typedef struct { uint8_t a, b, c; } Bytes3;     // 3 bytes

int32_t c_pair(Pair p) { return p.a * 10 + p.b; }
Pair c_make_pair(int32_t a, int32_t b) { Pair p = {a, b}; return p; }
int32_t c_triple(Triple t) { return t.a + t.b + t.c; }
Triple c_make_triple(int32_t v) { Triple t = {v, v + 1, v + 2}; return t; }
float c_vec3(Vec3 v) { return v.x + v.y + v.z; }
Vec3 c_make_vec3(float s) { Vec3 v = {s, s * 2, s * 3}; return v; }
double c_vec2d(Vec2d v) { return v.x * v.y; }
Vec2d c_make_vec2d(double a) { Vec2d v = {a, a + 1}; return v; }
int64_t c_big(Big b) { return b.a + b.b + b.c; }
Big c_make_big(int64_t v) { Big b = {v, v * 2, v * 3}; return b; }
double c_mixed(Mixed m) { return m.d + m.i; }
Mixed c_make_mixed(double d, int32_t i) { Mixed m = {d, i}; return m; }
int64_t c_wide(Wide w) { return (int64_t)(w.v >> 64) + (int64_t)w.v; }
int32_t c_bytes3(Bytes3 b) { return b.a + b.b * 2 + b.c * 3; }
int32_t c_small(int8_t a, uint8_t b, int16_t c, bool d) { return a + b + c + (d ? 1000 : 0); }
// After seven doubles, a float aggregate no longer fits the remaining
// registers and must travel wholly on the stack.
double c_many_fp(double a, double b, double c, double d, double e, double f, double g, Vec3 v) {
    return a + b + c + d + e + f + g + v.x + v.y + v.z;
}
// Likewise a two-register composite after seven integer arguments.
int32_t c_many_gp(int32_t a, int32_t b, int32_t c, int32_t d, int32_t e, int32_t f, int32_t g, Triple t) {
    return a + b + c + d + e + f + g + t.a + t.b + t.c;
}

extern int32_t fib_pair(Pair p);
extern Triple fib_triple(int32_t v);
extern float fib_vec3(Vec3 v);
extern int64_t fib_big(Big b);
extern Big fib_make_big(int64_t v);
int32_t c_callbacks(void) {
    Pair p = {4, 2};
    Vec3 v = {1.5f, 2.5f, 3.0f};
    Big b = {1, 2, 3};
    Triple t = fib_triple(5);
    Big m = fib_make_big(7);
    if (fib_pair(p) != 42) return 1;
    if (t.a != 5 || t.b != 6 || t.c != 7) return 2;
    if (fib_vec3(v) != 7.0f) return 3;
    if (fib_big(b) != 6) return 4;
    if (m.a != 7 || m.b != 14 || m.c != 21) return 5;
    return 0;
}
