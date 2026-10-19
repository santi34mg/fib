use crate::frontend::{
    analysis_annotation::BindingAnnotation, ast::type_expression::TypeExpression,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FunctionParameter {
    pub parameter_identifier: String,
    pub annotation: BindingAnnotation,
    pub parameter_type: TypeExpression,
}
