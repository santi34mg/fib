use crate::frontend::identifier::Identifier;

#[derive(Debug, Clone)]
pub enum Pattern {
    /// `.Variant` or `.Variant(binding)` — matches an enum variant by name.
    /// When `binding` is `Some`, a payload-carrying variant binds the payload
    /// fields under that local name (accessible as a struct).
    EnumVariant {
        variant: Identifier,
        binding: Option<Identifier>,
    },
    Wildcard,
}

