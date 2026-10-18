use crate::diagnostics::Span;
use crate::frontend::ast::function_declaration::FunctionDeclaration;
use crate::frontend::ast::{
    expression::Expression, switch::SwitchArm, variable_declaration::VariableDeclaration,
};

/// A statement together with the source position it starts on, used to point
/// analysis errors at the relevant spot in the source.
#[derive(Debug, Clone)]
pub struct Statement {
    pub kind: StatementKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum StatementKind {
    FunctionDeclaration(FunctionDeclaration),
    VariableDeclaration(VariableDeclaration),
    ExpressionStatement(Expression),
    Assignment {
        left: Expression,
        right: Expression,
    },
    DerefAssign {
        pointer: Expression,
        expr: Expression,
    },
    IndexAssign {
        object: Expression,
        index: Expression,
        expr: Expression,
    },
    Return(Option<Vec<Expression>>),
    MultiAssignment {
        targets: Vec<Expression>,
        values: Vec<Expression>,
    },
    // Separate bindings from values because rhs can be an expression that _semantically_
    // represents multiple values. example:
    // ```
    // q, r := divmod(5, 2);
    // ```
    MultiVariableDeclaration {
        bindings: Vec<VariableDeclaration>,
        /// Empty when all bindings are typed and no initializer was provided.
        values: Vec<Expression>,
    },
    If {
        condition: Expression,
        then_branch: Vec<Statement>,
        else_branch: Option<Vec<Statement>>,
    },
    While {
        condition: Option<Expression>,
        body: Vec<Statement>,
    },
    Break,
    Continue,
    Defer(Box<Statement>),
    Switch {
        subject: Expression,
        arms: Vec<SwitchArm>,
    },
}
