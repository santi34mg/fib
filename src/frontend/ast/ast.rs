use crate::frontend::ast::declaration::DeclarationNode;

#[derive(Debug, Clone)]
pub struct Ast {
    pub declarations: Vec<DeclarationNode>,
}

impl Ast {
    pub fn new() -> Self {
        Self {
            declarations: Vec::new(),
        }
    }
}

impl Default for Ast {
    fn default() -> Self {
        Self::new()
    }
}

