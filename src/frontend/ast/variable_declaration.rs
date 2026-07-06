use crate::frontend::{
    ast::{expression::Expression, type_expression::TypeExpression},
    identifier::Identifier,
};

#[derive(Debug, Clone)]
pub struct VariableDeclaration {
    pub identifier: Identifier,
    pub constant_type: Option<TypeExpression>,
    pub expression: Option<Expression>,
}

impl VariableDeclaration {
    pub fn new(
        identifier: Identifier,
        constant_type: Option<TypeExpression>,
        expression: Option<Expression>,
    ) -> Self {
        Self {
            identifier,
            constant_type,
            expression,
        }
    }
}
