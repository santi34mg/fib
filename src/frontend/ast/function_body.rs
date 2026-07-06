use crate::frontend::ast::statement::Statement;


#[derive(Debug, Clone)]
pub struct FunctionBody {
    pub statements: Vec<Statement>,
}
