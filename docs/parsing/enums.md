# Enums and payload variants

> **Verified parser behavior:** Enum type declarations, member expressions, and switch patterns parse.

## Declaring enum types

```fib
Color: type = enum {
    Red,
    Green,
    Blue,
};

Token: type = enum {
    Integer { value: uint32 },
    Boolean { flag: bool },
    EOF,
};
```

A variant is a name with an optional brace-delimited list of named, typed fields.
Commas and trailing commas are accepted. An empty payload field list also parses.
The declaration is a unified binding with a `TypeValue` initializer and a
required trailing semicolon: `Name: type = enum { ... };`. Explicit numeric tags,
underlying-tag annotations, and tuple-style payload declarations are not parsed.

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
        when .Integer(payload) { value := payload.value; }
        when .Boolean(payload) { flag := payload.flag; }
        when .EOF { }
    };
}
```

Patterns record variant names and optional payload binding names. See
[Switch](switch.md) for wildcard syntax and accepted pattern forms.
Constructor and member design is discussed in [Name resolution](../name-resolution.md)
and the [roadmap](../roadmap/).

Parser coverage: `type_declarations_preserve_struct_and_enum_payload_shapes`,
`switch_payload_binding_preserves_variant_and_body`, and
`parsed_pattern_payload_and_variant_annotations_are_pending`.
