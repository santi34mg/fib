use crate::frontend::next_ast::statement::Statement;

#[derive(Debug, Clone)]
pub struct FunctionBody {
    pub statements: Vec<Statement>,
}
