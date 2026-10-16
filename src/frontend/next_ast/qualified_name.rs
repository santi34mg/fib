use crate::frontend::tokens::name::Name;

#[derive(Debug, Clone, PartialEq)]
pub struct QualifiedName {
    pub segments: Vec<Name>,
}
