use core::fmt;

use crate::token::builtin::BuiltinType;
use crate::token::Operator;
use crate::token::identifier::Identifier;
use crate::token::literal::Literal;

#[derive(Debug, Clone)]
pub struct Ast {
    pub declarations: Vec<DeclarationNode>,
}

impl Ast {
    pub fn new() -> Self {
        return Self {
            declarations: Vec::new(),
        };
    }
}


#[derive(Debug, Clone)]
pub enum DeclarationNode {
    FunctionDeclaration(FunctionDeclaration),
    TypeDeclaration(TypeDeclaration),
    Statement(StatementNode),
}

#[derive(Debug, Clone)]
pub enum StatementNode {
    ConstantDeclaration(ConstantDeclaration),
    ExpressionStatement(Expression),
    Assignment {
        identifier: Identifier,
        expr: Expression,
    },
    Return(Option<Expression>),
    If {
        condition: Expression,
        then_branch: Vec<StatementNode>,
        else_branch: Option<Vec<StatementNode>>,
    },
    For {
        initializer: Option<Box<StatementNode>>,
        condition: Option<Expression>,
        increment: Option<Box<StatementNode>>,
        body: Vec<StatementNode>,
    },
}

#[derive(Debug, Clone)]
pub struct FunctionDeclaration {
    pub signature: FunctionSignature,
    pub body: FunctionBody,
}

#[derive(Debug, Clone)]
pub struct FunctionSignature {
    pub name: Identifier,
    pub parameters: Vec<FunctionParameter>,
    pub return_type: Option<TypeExpression>,
}

#[derive(Debug, Clone)]
pub struct FunctionParameter {
    pub parameter_name: Identifier,
    pub parameter_type: TypeExpression,
}

#[derive(Debug, Clone)]
pub struct FunctionBody {
    pub statements: Vec<StatementNode>,
}

#[derive(Debug, Clone)]
pub struct TypeDeclaration {
    pub name: Identifier,
    pub type_expression: TypeExpression,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeExpression {
    Builtin(BuiltinType),
    Identifier(Identifier),
    Function {
        argument_types: Vec<TypeExpression>,
        return_type: Box<TypeExpression>,
    },
    Pointer {
        pointer_variant: PointerVariant,
        pointed_type: Box<TypeExpression>,
    },
    Struct {
        fields: Vec<Field>,
    },
}

impl fmt::Display for TypeExpression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeExpression::Builtin(builtin_type) => {
                write!(f, "{}", builtin_type)?;
            }
            TypeExpression::Identifier(identifier) => {
                write!(f, "{}", identifier)?;
            }
            TypeExpression::Function { argument_types, return_type } => {
                write!(f, "function({:?}) -> {}", argument_types, return_type)?;
            }
            TypeExpression::Struct { fields } => {
                write!(f, "struct {{ {:?} }}", fields)?;
            }
            TypeExpression::Pointer { pointer_variant, pointed_type } => {
                match pointer_variant {
                    PointerVariant::Unique => {
                        write!(f, "unique &{}", *pointed_type)?;
                    }
                    PointerVariant::Shared => {
                        write!(f, "shared &{}", *pointed_type)?;
                    }
                    PointerVariant::Weak => {
                        write!(f, "weak &{}", *pointed_type)?;
                    }
                    PointerVariant::Raw => {
                        write!(f, "&{}", *pointed_type)?;
                    }
                }
            }
        }
        Ok(())
    }
}


#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub(crate) label: Identifier,
    pub(crate) type_id: TypeExpression,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PointerVariant {
    Unique,
    Shared,
    Weak,
    Raw,
}

#[derive(Debug, Clone)]
pub struct ConstantDeclaration {
    pub identifier: Identifier,
    pub constant_type: Option<TypeExpression>,
    pub expression: Option<Expression>,
}

impl ConstantDeclaration {
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


#[derive(Debug, Clone)]
pub enum Expression {
    Binary {
        left: Box<Expression>,
        operator: Operator,
        right: Box<Expression>,
    },
    Unary {
        operator: Operator,
        expression: Box<Expression>,
    },
    Literal(Literal),
    // The identifier expression contains the name of the identifier as a string
    Identifier(Identifier),
    Grouping(Box<Expression>),
    Call {
        callee: Box<Expression>,
        args: Vec<Expression>,
    },
}
