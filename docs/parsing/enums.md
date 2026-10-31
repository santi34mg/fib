# Enums and payload variants

Construction and payload-copy semantics are recorded in [enum semantics](../enum-semantics.md).
The confirmed design requires exhaustive switches, using `when else` as default;
analysis enforces coverage, including unreachable switches.

> **Verified parser behavior:** Enum type declarations, member expressions, and switch patterns parse.

## Declaring enum types

```fib
comp Color: type = enum {
    Red,
    Green,
    Blue,
};

comp Token: type = enum {
    Integer: uint32,
    Boolean: bool,
    EOF,
};
```

A variant is a name with an optional `: Type` payload. Payloads retain ordinary
type-valued expressions, including aliases, computed types and explicit structs
(`Pair: struct { left: int32, right: int32 }`). Commas and trailing commas are
accepted. The former `Variant { field: Type }` syntax reports a migration error.
The declaration is a unified binding with a `TypeValue` initializer and a
required trailing semicolon: `comp Name: type = enum { ... };`. Explicit numeric
tags, underlying-tag annotations and `Variant(Type)` declarations are not parsed.
Missing closing enum braces are rejected.

## Member expressions are not resolved constructors

```fib
color := Color.Green;
constructor_call := Token.Integer(7);
```

`Color.Green` is an ordinary member-access AST node, not a resolved enum value.
`Token.Integer(7)` is an ordinary call whose callee is a member expression. The
parser preserves those shapes without deciding whether a member is a variant,
field, callable, or error.

The former payload construction spelling `Token.Integer { value: 7 }` is **not
parsed**: braces following a dotted member are not consumed as a payload.
The AST currently has no dedicated enum-construction expression. Casting
`color as uint64` parses as an ordinary cast expression.

## Matching syntax

```fib
fn inspect(token: Token) void {
    switch (token) {
        when .Integer(payload) { value := payload; }
        when .Boolean(payload) { flag := payload; }
        when .EOF { }
    };
}
```

Patterns record variant names and optional payload binding names. Semantic analysis
gives normalized unit variants optional void bindings and requires bare-value
construction; the parser preserves ordinary member/call shapes. See
[Switch](switch.md) for wildcard syntax and accepted pattern forms.
Constructor and member design is discussed in [Name resolution](../name-resolution.md)
and the [roadmap](../roadmap/).

Parser coverage: `type_declarations_preserve_struct_and_enum_payload_shapes`,
`direct_enum_payload_syntax_and_legacy_errors`,
`switch_payload_binding_preserves_variant_and_body`, and
`parsed_pattern_payload_and_variant_annotations_are_pending`.
