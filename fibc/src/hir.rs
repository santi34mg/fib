use core::fmt;
use std::collections::HashMap;

use crate::token::{builtin::BuiltinType, identifier::Identifier, Operator};

#[derive(Debug, Clone)]
pub struct CompilationUnit {
    pub scope_root: Scope,
    pub declarations: Vec<HIRDeclaration>
}

impl CompilationUnit {
    pub fn new() -> Self {
        return Self {
            scope_root: Scope::new(),
            declarations: Vec::new(),
        };
    }
}

#[derive(Debug, Clone)]
pub struct Scope {
    pub symbols: HashMap<Identifier, HIRSymbol>,
    pub children_scope: Vec<Box<Scope>>,
}

impl Scope {
    pub fn new() -> Self {
        return Self {
            symbols: HashMap::new(),
            children_scope: Vec::new(),
        };
    }
}

#[derive(Debug, Clone)]
pub enum HIRSymbol {
    Type(HIRTypeKind),
    Function(HIRFunction),
    Variable(HIRVar),
}

#[derive(Debug, Clone, PartialEq)]
pub enum HIRTypeKind {
    Builtin(BuiltinType),
    Identifier(Identifier),
    Struct,
}

impl fmt::Display for HIRTypeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Builtin(builtin) => write!(f, "{}", builtin)?,
            _ => todo!()
        };
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum HIRDeclaration {
    HIRFunction(HIRFunction),
    HIRVar(HIRVar),
}

#[derive(Debug, Clone)]
pub struct HIRFunction {
    pub name: Identifier,
    pub params: Vec<(Identifier, HIRTypeKind)>,
    pub return_type: HIRTypeKind,
    pub body: Vec<HIRStmt>,
}

#[derive(Debug, Clone)]
pub struct HIRExpression {
    pub inferred_type: HIRTypeKind,
    pub expression: HIRExpressionKind,
}

#[derive(Debug, Clone)]
pub enum HIRExpressionKind {
    LiteralInt{
        value: u64,
    },
    LiteralBool(bool),
    Variable(Identifier),
    Binary {
        left: Box<HIRExpression>,
        operator: Operator,
        right: Box<HIRExpression>,
    },
    Call {
        callee: Identifier,
        args: Vec<HIRExpression>,
    },
    Null,
}

#[derive(Debug, Clone)]
pub enum HIRStmt {
    Let(HIRVar),
    Assign {
        name: Identifier,
        expr: HIRExpression,
    },
    Expr(HIRExpression),
    Return(Option<HIRExpression>),
    If {
        cond: HIRExpression,
        then_branch: Vec<HIRStmt>,
        else_branch: Option<Vec<HIRStmt>>,
    },
    For {
        init: Option<Box<HIRStmt>>,
        cond: Option<HIRExpression>,
        post: Option<Box<HIRStmt>>,
        body: Vec<HIRStmt>,
    },
}

#[derive(Debug, Clone)]
pub struct HIRVar {
    pub name: Identifier,
    pub ty: HIRTypeKind,
    pub init: Option<HIRExpression>,
}
