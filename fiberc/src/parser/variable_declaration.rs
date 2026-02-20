use crate::parser::{TypeIdentifier, expression::Expression};

#[derive(Debug, Clone)]
pub struct VariableDeclaration {
    pub identifier: String,
    pub variable_type: Option<TypeIdentifier>,
    pub expression: Option<Expression>,
}

impl VariableDeclaration {
    pub fn new(
        identifier: String,
        variable_type: Option<TypeIdentifier>,
        expression: Option<Expression>,
    ) -> Self {
        Self {
            identifier,
            variable_type,
            expression,
        }
    }
}
