use crate::frontend::ast::{pattern::Pattern, statement::Statement};

/// A statement together with the source line it starts on, used to point
/// analysis errors at the relevant line of source code.
#[derive(Debug, Clone)]
pub struct SwitchArm {
    pub pattern: Pattern,
    pub body: Vec<Statement>,
}

