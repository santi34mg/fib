use crate::frontend::{analysis_annotation::MemberAnnotation, ast::expression::Expression};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StructField {
    pub field_identifier: String,
    /// A member of the owning type, for both definitions and initializer fields;
    /// never a lexical binding.
    pub annotation: MemberAnnotation,
    pub expression: Expression,
}
