use crate::frontend::{
    ast::{expression::Expression, switch::SwitchArm, variable_declaration::VariableDeclaration}, identifier::Identifier,
};

/// A statement together with the source line it starts on, used to point
/// analysis errors at the relevant line of source code.
#[derive(Debug, Clone)]
pub struct Statement {
    pub kind: StatementKind,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub enum StatementKind {
    VariableDeclaration(VariableDeclaration),
    ExpressionStatement(Expression),
    Assignment {
        identifier: Identifier,
        expr: Expression,
    },
    FieldAssign {
        object: Expression,
        field: Identifier,
        expr: Expression,
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
    MultiVariableDeclaration {
        identifiers: Vec<Identifier>,
        values: Vec<Expression>,
    },
    If {
        condition: Expression,
        then_branch: Vec<Statement>,
        else_branch: Option<Vec<Statement>>,
    },
    For {
        initializer: Option<Box<Statement>>,
        condition: Option<Expression>,
        post_operation: Option<Box<Statement>>,
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
