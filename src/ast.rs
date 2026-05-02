use core::fmt;

use crate::tokens::Operator;
use crate::tokens::builtin::BuiltinType;
use crate::tokens::identifier::Identifier;
use crate::tokens::literal::Literal;

pub type ModulePath = Vec<Identifier>;

#[derive(Debug, Clone)]
pub struct ImportDeclaration {
    pub path: ModulePath,
    pub alias: Option<Identifier>,
    pub selective: Option<Vec<Identifier>>,
}

#[derive(Debug, Clone)]
pub struct Ast {
    pub declarations: Vec<DeclarationNode>,
}

impl Ast {
    pub fn new() -> Self {
        Self {
            declarations: Vec::new(),
        }
    }
}

impl Default for Ast {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub enum DeclarationNode {
    ImportDeclaration(ImportDeclaration),
    FunctionDeclaration(FunctionDeclaration),
    TypeDeclaration(TypeDeclaration),
}

#[derive(Debug, Clone)]
pub struct TypeDeclaration {
    pub name: Identifier,
    pub expression: TypeExpression,
}

#[derive(Debug, Clone)]
pub enum StatementNode {
    VariableDeclaration(VariableDeclaration),
    ExpressionStatement(Expression),
    Assignment {
        identifier: Identifier,
        expr: Expression,
    },
    FieldAssign {
        object: Expression,
        field: Identifier,
        expr: Expression,
    },
    DerefAssign {
        pointer: Expression,
        expr: Expression,
    },
    IndexAssign {
        object: Expression,
        index: Expression,
        expr: Expression,
    },
    Return(Option<Vec<Expression>>),
    MultiAssignment {
        targets: Vec<Expression>,
        values: Vec<Expression>,
    },
    MultiVariableDeclaration {
        identifiers: Vec<Identifier>,
        values: Vec<Expression>,
    },
    If {
        condition: Expression,
        then_branch: Vec<StatementNode>,
        else_branch: Option<Vec<StatementNode>>,
    },
    For {
        initializer: Option<Box<StatementNode>>,
        condition: Option<Expression>,
        post_operation: Option<Box<StatementNode>>,
        body: Vec<StatementNode>,
    },
    Break,
    Continue,
    Defer(Box<StatementNode>),
    Switch {
        subject: Expression,
        arms: Vec<SwitchArm>,
    },
}

#[derive(Debug, Clone)]
pub struct SwitchArm {
    pub pattern: Pattern,
    pub body: Vec<StatementNode>,
}

#[derive(Debug, Clone)]
pub enum Pattern {
    /// `.Variant` or `.Variant(binding)` — matches an enum variant by name.
    /// When `binding` is `Some`, a payload-carrying variant binds the payload
    /// fields under that local name (accessible as a struct).
    EnumVariant {
        variant: Identifier,
        binding: Option<Identifier>,
    },
    Wildcard,
}

#[derive(Debug, Clone)]
pub struct FunctionDeclaration {
    pub signature: FunctionSignature,
    pub body: Option<FunctionBody>,
    pub is_extern: bool,
    pub is_variadic: bool,
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

#[derive(Debug, Clone, PartialEq)]
pub enum TypeExpression {
    Builtin(BuiltinType),
    Identifier(Identifier),
    Function {
        argument_types: Vec<TypeExpression>,
        return_type: Box<TypeExpression>,
    },
    Tuple {
        elements: Vec<TypeExpression>,
    },
    Pointer {
        pointer_variant: PointerVariant,
        pointed_type: Box<TypeExpression>,
    },
    Struct {
        fields: Vec<Field>,
    },
    Enum {
        variants: Vec<EnumVariant>,
    },
    Array {
        element_type: Box<TypeExpression>,
        size: u64,
    },
    QualifiedIdentifier {
        module: Identifier,
        name: Identifier,
    },
    /// The `type` keyword used as a type annotation — indicates this binding holds a compile-time type value.
    TypeKeyword,
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
            TypeExpression::Function {
                argument_types,
                return_type,
            } => {
                write!(f, "function({:?}) -> {}", argument_types, return_type)?;
            }
            TypeExpression::Struct { fields } => {
                write!(f, "struct {{ {:?} }}", fields)?;
            }
            TypeExpression::Tuple { elements } => {
                write!(f, "({:?})", elements)?;
            }
            TypeExpression::Enum { variants } => {
                write!(f, "enum {{ {:?} }}", variants)?;
            }
            TypeExpression::Array { element_type, size } => {
                write!(f, "{}[{}]", element_type, size)?;
            }
            TypeExpression::QualifiedIdentifier { module, name } => {
                write!(f, "{}::{}", module, name)?;
            }
            TypeExpression::TypeKeyword => {
                write!(f, "type")?;
            }
            TypeExpression::Pointer {
                pointer_variant,
                pointed_type,
            } => match pointer_variant {
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
                    write!(f, "*{}", *pointed_type)?;
                }
            },
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
pub struct EnumVariant {
    pub name: Identifier,
    pub payload: Option<Vec<Field>>,
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
    FieldAccess {
        object: Box<Expression>,
        field: Identifier,
    },
    AddressOf(Box<Expression>),
    Dereference(Box<Expression>),
    StructConstruct {
        type_name: Identifier,
        fields: Vec<(Identifier, Expression)>,
    },
    Cast {
        expr: Box<Expression>,
        target_type: TypeExpression,
    },
    IndexAccess {
        object: Box<Expression>,
        index: Box<Expression>,
    },
    ArrayLiteral {
        elements: Vec<Expression>,
    },
    QualifiedAccess {
        module: Identifier,
        member: Identifier,
    },
    /// A compile-time type value used in expression position (e.g., as a generic argument).
    TypeValue(TypeExpression),
    /// `Type.Variant { field: val, ... }` — construct an enum variant carrying
    /// a payload. For payload-less variants, use a plain `FieldAccess` instead.
    EnumVariantConstruct {
        type_name: Identifier,
        variant: Identifier,
        fields: Vec<(Identifier, Expression)>,
    },
}
