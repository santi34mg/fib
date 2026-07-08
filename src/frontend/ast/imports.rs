use crate::frontend::identifier::Identifier;

pub type ModulePath = Vec<Identifier>;

#[derive(Debug, Clone)]
pub struct ImportDeclaration {
    pub path: ModulePath,
    pub alias: Option<Identifier>,
    pub selective: Option<Vec<Identifier>>,
}
