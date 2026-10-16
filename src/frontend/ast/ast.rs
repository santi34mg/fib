use crate::frontend::ast::statement::Statement;

#[derive(Debug, Clone, Default)]
pub struct Ast {
    pub statements: Vec<Statement>,
}

impl Ast {
    pub fn new() -> Self {
        Self {
            statements: Vec::new(),
        }
    }
}

impl From<Vec<Statement>> for Ast {
    fn from(statements: Vec<Statement>) -> Self {
        Self { statements }
    }
}
