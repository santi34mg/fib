use crate::frontend::{
    analysis_annotation::BindingAnnotation, ast::type_expression::TypeExpression,
};

#[derive(Debug, Clone)]
pub struct TypeDeclaration {
    pub identifier: String,
    pub annotation: BindingAnnotation,
    pub expression: TypeExpression,
}
