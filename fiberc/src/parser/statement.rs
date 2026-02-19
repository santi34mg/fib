use crate::parser::VariableDeclaration;
use crate::parser::expression::Expression;
use crate::parser::function::Function;

#[derive(Debug, Clone)]
pub enum Statement {
    VariableDeclaration(VariableDeclaration),
    Assignment {
        identifier: String,
        expr: Expression,
    },
    Expression(Expression),
    FunctionDeclaration(Function),
    Return(Option<Expression>),
    If {
        condition: Expression,
        then_branch: Vec<Statement>,
        else_branch: Option<Vec<Statement>>,
    },
    For {
        initializer: Option<Box<Statement>>,
        condition: Option<Expression>,
        increment: Option<Box<Statement>>,
        body: Vec<Statement>,
    },
}
