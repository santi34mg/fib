use crate::frontend::analysis_annotation::{BindingAnnotation, MemberAnnotation};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PatternBinding {
    pub identifier: String,
    pub annotation: BindingAnnotation,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Pattern {
    /// `.Variant` or `.Variant(binding)` — matches an enum variant by name.
    /// When `binding` is `Some`, a payload-carrying variant binds the payload
    /// fields under that local name (accessible as a struct).
    EnumVariant {
        variant: String,
        member: MemberAnnotation,
        binding: Option<PatternBinding>,
    },
    Wildcard,
}
