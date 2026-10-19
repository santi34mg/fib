use crate::frontend::analysis_annotation::BindingAnnotation;
use crate::frontend::ast::{
    function_parameter::FunctionParameter, type_expression::TypeExpression,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FunctionSignature {
    pub identifier: String,
    /// The function's sole binding slot; FunctionDeclaration does not duplicate it.
    pub annotation: BindingAnnotation,
    pub parameters: Vec<FunctionParameter>,
    pub return_type: TypeExpression,
    /// Reserved syntax, currently never populated by the parser. Binding slots
    /// belong on parameter nodes once generic declaration syntax is defined.
    pub generic_parameters: Vec<String>,
}
