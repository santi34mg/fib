use crate::frontend::ast::statement::Statement;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FunctionBody {
    pub statements: Vec<Statement>,
}
