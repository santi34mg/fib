use crate::frontend::{
    analysis_annotation::BindingAnnotation,
    ast::{expression::Expression, type_expression::TypeExpression},
};

#[derive(Debug, Clone)]
pub struct VariableDeclaration {
    pub identifier: String,
    /// Also owns the binding identity when nested in a multi-declaration.
    pub annotation: BindingAnnotation,
    pub ty: Option<TypeExpression>,
    pub expression: Option<Expression>,
    pub is_const: bool,
}

impl VariableDeclaration {
    pub fn new(
        identifier: String,
        ty: Option<TypeExpression>,
        expression: Option<Expression>,
    ) -> Self {
        Self {
            identifier,
            annotation: BindingAnnotation::default(),
            ty,
            expression,
            is_const: false,
        }
    }
}
