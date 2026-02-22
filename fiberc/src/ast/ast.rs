#[derive(Debug, Clone)]
pub struct Ast {
    pub program: ProgramNode,
}

impl Ast {
    pub fn new() -> Self {
        return Self {
            program: ProgramNode {
                modules: Vec::new(),
            },
        };
    }
}

#[derive(Debug, Clone)]
pub struct ProgramNode {
    pub modules: Vec<ModuleNode>,
}

#[derive(Debug, Clone)]
pub struct ModuleNode {
    pub declarations: Vec<DeclarationNode>,
}

#[derive(Debug, Clone)]
pub enum DeclarationNode {
    FunctionDeclaration(FunctionDeclaration),
    TypeDeclaration(TypeDeclaration),
    ModuleDeclaration(ModuleDeclaration),
    Statement(StatementNode),
}

#[derive(Debug, Clone)]
pub struct ModuleDeclaration {
    pub name: String,
    pub uses: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum StatementNode {
    VariableDeclaration(VariableDeclaration),
    ExpressionStatement(Expression),
    Assignment {
        identifier: String,
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
    pub body: Option<FunctionBody>,
}

#[derive(Debug, Clone)]
pub struct FunctionSignature {
    pub name: String,
    pub parameters: Vec<FunctionParameter>,
    pub return_type: Option<TypeIdentifier>,
}

#[derive(Debug, Clone)]
pub struct FunctionParameter {
    pub parameter_name: String,
    pub parameter_type: TypeIdentifier,
}

#[derive(Debug, Clone)]
pub enum FunctionBody {
    Statements(Vec<StatementNode>),
}

#[derive(Debug, Clone)]
pub struct TypeDeclaration {}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeIdentifier {
    UserDefinedType(String),
    Unit,
    Integer,
    Float,
    String,
    Boolean,
    Character,
    Function {
        argument_types: Vec<TypeIdentifier>,
        return_type: Box<TypeIdentifier>,
    },
    Pointer {
        pointer_variant: PointerVariant,
        pointed_type: Box<TypeIdentifier>,
    },
    Struct {
        fields: Vec<Field>,
    },
    Variant {
        fields: Vec<Field>,
    },
    Dynamic,
    Blob,
    Never,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub(crate) label: String,
    pub(crate) type_id: TypeIdentifier,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PointerVariant {
    Unique,
    Shared,
    Weak,
    Raw,
}

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

use crate::token::Operator;
use crate::token::literal::Literal;

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
    Identifier(String),
    Grouping(Box<Expression>),
    Call {
        callee: Box<Expression>,
        args: Vec<Expression>,
    },
}
