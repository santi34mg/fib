pub mod binary_expression;

pub mod enum_variant;
pub mod expression;
pub mod function_body;
pub mod function_declaration;
pub mod function_parameter;
pub mod function_signature;

pub mod pattern;
pub mod qualified_name;
pub mod statement;
pub mod structl_field;
pub mod switch;
pub mod type_expression;
pub mod unary_expression;
pub mod variable_declaration;

use statement::Statement;

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Ast {
    pub statements: Vec<Statement>,
}

impl Ast {
    pub fn new() -> Self {
        Self {
            statements: Vec::new(),
        }
    }
}

impl From<Vec<Statement>> for Ast {
    fn from(statements: Vec<Statement>) -> Self {
        Self { statements }
    }
}
