# Fiber Language MVP Completion Plan

## Context

Fiber needs to be usable enough to start writing a self-hosted compiler. The main gaps are: syntax inconsistencies (parameter/field ordering differs from variable declarations), missing type system features (enums, pattern matching), and weak type checking. The user wants to focus on finishing the type system and fixing syntax, with minimal IO handled through libc and the current module system kept as-is.

---

## DONE - Phase 0: Syntax Consistency — C-style `TYPE NAME` Everywhere

**Problem**: Variable declarations use `var TYPE NAME` but function parameters use `NAME TYPE` and struct fields use `NAME TYPE`. User chose C-style ordering everywhere.

### DONE - 0a. Change function parameter syntax to `TYPE NAME`

**Files**:
- `src/parser/parser.rs` — In the function parameter parsing (~line 1389), currently parses identifier first then type. Reverse to: call `parse_type()` first, then `expect_identifier()`.
- `src/integration_tests.rs` — Update all test source strings
- `../samples/**/*.fib` — Update all sample files
- `../std/libc.fib` — Update extern declarations

No AST/HIR/analysis/lowering changes needed — `FunctionParameter` already stores `parameter_name` and `parameter_type` as separate fields.

### DONE - 0b. Change struct field syntax to `TYPE NAME`

**Files**:
- `src/parser/parser.rs` — In `parse_type_fields()` (~line 763), currently parses identifier then type. Reverse to: call `parse_type()` first, then `expect_identifier()`.
- `src/integration_tests.rs` — Update test source strings
- `../samples/**/*.fib` — Update sample files

No AST/HIR/analysis/lowering changes needed — struct field tuples already store name and type separately.

---

## Phase 1: Syntactic Quality-of-Life

### DONE - 1a. `while` loop (sugar over existing `for`)


Desugar `while (cond) { body }` into `For { init: None, cond: Some(cond), post: None, body }`.

**Files**:
- `src/token/keyword.rs` — Add `While` variant
- `src/lexer/lexer.rs` — Map `"while"` to `Keyword::While`
- `src/parser/parser.rs` — Add `Keyword::While` arm in `parse_statement()`, parse `(cond)` then body, emit as `For` node

No AST/HIR/analysis/lowering changes.

### 1b. `else if` chaining

Currently requires `else { if ... { } }`. Allow `else if` without extra braces.

**Files**:
- `src/parser/parser.rs` — In `if` parsing (~line 198), after consuming `else`, if next token is `Keyword::If`, recursively parse as `if` statement and wrap in a single-element else branch body.

No other changes needed.

### 1c. Multi-level LHS assignment

Currently `x.field = val` works but `x.field1.field2 = val`, `x.*.field = val` don't.

**Approach**: Parse LHS as a full expression, then check for `=` / compound assignment. Determine assignment kind from expression shape.

**Files**:
- `src/ast/ast.rs` — Change `FieldAssign` to take `object: Expression` instead of `object: Identifier`. Same for compound assignment targets.
- `src/parser/parser.rs` — Rewrite the identifier-starts-a-statement branch (~lines 265-420): parse full expression first, then check for assignment operators. Emit appropriate assignment node based on expression shape (FieldAccess → FieldAssign, Dereference → DerefAssign, IndexAccess → IndexAssign, Identifier → Assignment).
- `src/hir.rs` — Change `HIRStmt::FieldAssign` object to `HIRExpression`
- `src/analysis/generate_hir.rs` — Update FieldAssign handling to resolve types through expression chains
- `src/lowering/llvm_lower.rs` — Emit GEP chains for nested field access on LHS. Handle compound assignment on non-identifiers (desugar `lhs op= rhs` to `lhs = lhs op rhs`).

### 1d. AddressOf for non-identifiers

Currently `.&` only works on plain identifiers in lowering.

**Files**:
- `src/lowering/llvm_lower.rs` — In `AddressOf` handling, if inner expression is FieldAccess or IndexAccess, emit GEP and return the pointer directly instead of looking up a named variable.

---

## Phase 2: Enums and Pattern Matching

### 2a. C-style enums (no payload)

```
const type Color = enum {
    Red,
    Green,
    Blue,
}
```

Variants get integer discriminants starting from 0. Access via `Color.Red`.

**Pipeline changes**:
- `src/token/keyword.rs` — `Enum` already exists
- `src/lexer/lexer.rs` — `enum` already lexed
- `src/ast/ast.rs` — Add `TypeExpression::Enum { variants: Vec<EnumVariant> }`. Add `struct EnumVariant { name: Identifier, payload: Option<Vec<Field>> }`.
- `src/parser/parser.rs` — In `parse_type()`, add `Keyword::Enum` arm. Parse `enum { Ident, Ident, ... }`.
- `src/hir.rs` — Add `HIRTypeKind::Enum { variants: Vec<HIREnumVariant> }`. Add `HIRExpressionKind::EnumLiteral { type_name, variant, discriminant }`.
- `src/analysis/generate_hir.rs` — Handle `TypeExpression::Enum` in `map_type()`. When resolving `FieldAccess` on a type name that's an enum, emit `EnumLiteral`. Register enum type in scope.
- `src/lowering/llvm_lower.rs` — Map `HIRTypeKind::Enum` to `i32`. Emit `EnumLiteral` as integer constant.

### 2b. switch/when (pattern matching on enums)

```
switch (color) {
    when .Red { ... }
    when .Green { ... }
    when .Blue { ... }
}
```

**Pipeline changes**:
- `src/ast/ast.rs` — Add `StatementNode::Switch { subject: Expression, arms: Vec<SwitchArm> }`. Add `struct SwitchArm { pattern: Pattern, body: Vec<StatementNode> }`. Add `enum Pattern { EnumVariant(Identifier), Literal(Literal), Wildcard }`.
- `src/parser/parser.rs` — Add `Keyword::Switch` arm. Parse `switch (expr) { when pattern { body } ... }`.
- `src/hir.rs` — Add `HIRStmt::Switch { subject: HIRExpression, arms: Vec<HIRSwitchArm> }`.
- `src/analysis/generate_hir.rs` — Resolve switch subject type, resolve each pattern to a discriminant.
- `src/lowering/llvm_lower.rs` — Use LLVM `switch` instruction with basic blocks per arm.

### 2c. Tagged unions (enum variants with payload)

```
const type Token = enum {
    Integer { value int64 },
    Identifier { name string },
    EOF,
}

switch (tok) {
    when .Integer(i) { printf("%d", i.value) }
    when .Identifier(id) { puts(id.name) }
    when .EOF { return }
}
```

LLVM representation: `{ i32 tag, [N x i8] payload }` where N = size of largest variant.

**Extends 2a/2b**:
- `EnumVariant.payload` becomes `Some(fields)` for data-carrying variants
- Pattern matching binds payload fields: `when .Variant(binding)` bitcasts payload to variant struct
- Lowering: construct tagged unions by writing tag + bitcasting payload region. Destruct by checking tag + bitcasting to read.

---

## Phase 3: Basic Type Checking

All changes in `src/analysis/generate_hir.rs`:

- **3a. Function call argument types** — After resolving callee, compare each argument's inferred type against declared parameter types
- **3b. Return type checking** — Verify returned expression type matches function's declared return type
- **3c. Assignment type checking** — Verify RHS type matches target type for all assignment forms
- **3d. Binary operator type compatibility** — Verify LHS/RHS types are compatible instead of blindly coercing

---

## Phase 4: Cleanup

- **4a. sint32 alias** — Add `sint8/16/32/64` as aliases in `src/lexer/lexer.rs` mapping to `Int8/16/32/64`, or migrate everything to `int` and remove `sint` from samples/docs
- **4b. Remove dead pointer variants** — Strip `Unique`/`Shared`/`Weak` from `PointerVariant` enum, keep only `Raw`. Remove dead `parse_pointer()` method.
- **4c. Remove unused tokens** — Remove or document `DoubleDot` (..), `At` (@) if not planned for use. Remove `Float16`/`Float128` from `BuiltinType` if no lexer mapping planned.

---

## What to SKIP for MVP

- **Generic structs** — Hand-roll per-type (e.g., `TokenList`, `NodeList`)
- **Traits/interfaces/methods** — Use free functions with explicit params
- **Closures/lambdas** — Not needed
- **Ownership semantics** — Raw pointers + manual malloc/free
- **String type improvements** — Use C strings + manual buffer management
- **While with no parens** — Keep `while (cond)` with required parens for now

---

## Verification

After each phase:
1. `cargo test -p fibc` — All existing integration tests pass
2. Compile sample programs: `cd samples/hello_world && fiber compile && ./out/hello_world`
3. Write new sample programs exercising added features (e.g., `samples/enums/` for Phase 2)
4. After all phases: write a minimal Fiber program that lexes a simple input string as a smoke test for self-hosting readiness
