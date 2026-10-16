use crate::frontend::{
    next_ast::{expression::Expression, type_expression::TypeExpression},
    tokens::name::Name,
};

#[derive(Debug, Clone)]
pub struct VariableDeclaration {
    pub name: Name,
    pub ty: Option<TypeExpression>,
    pub expression: Option<Expression>,
    pub is_const: bool,
}

impl VariableDeclaration {
    pub fn new(name: Name, ty: Option<TypeExpression>, expression: Option<Expression>) -> Self {
        Self {
            name,
            ty,
            expression,
            is_const: false,
        }
    }
}
