use crate::frontend::ast::structl_field::StructField;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EnumVariant {
    pub identifier: String,
    pub payload: Option<Vec<StructField>>,
}
