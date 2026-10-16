use crate::frontend::{next_ast::structl_field::StructField, tokens::name::Name};

#[derive(Debug, Clone, PartialEq)]
pub struct EnumVariant {
    pub name: Name,
    pub payload: Option<Vec<StructField>>,
}

/*
type Colors = enum {
    Red[]
}
*/
