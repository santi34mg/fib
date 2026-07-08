use crate::frontend::ast::{
    function_declaration::FunctionDeclaration, imports::ImportDeclaration,
    type_declaration::TypeDeclaration,
};

#[derive(Debug, Clone)]
pub enum DeclarationNode {
    ImportDeclaration(ImportDeclaration),
    FunctionDeclaration(FunctionDeclaration),
    TypeDeclaration(TypeDeclaration),
}
