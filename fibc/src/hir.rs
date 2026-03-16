use core::fmt;
use std::collections::HashMap;

use crate::token::{Operator, builtin::BuiltinType, identifier::Identifier};

#[derive(Debug, Clone)]
pub struct CompilationUnit {
    pub scope_root: Scope,
    pub declarations: Vec<HIRDeclaration>,
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
    Binding(HIRBinding),
}

#[derive(Debug, Clone, PartialEq)]
pub enum HIRTypeKind {
    Builtin(BuiltinType),
    Identifier(Identifier),
    Struct { fields: Vec<(String, Box<HIRTypeKind>)> },
}

impl fmt::Display for HIRTypeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Builtin(builtin) => write!(f, "{}", builtin)?,
            Self::Identifier(id) => write!(f, "{}", id)?,
            Self::Struct { fields } => write!(f, "struct {{ {:?} }}", fields)?,
        };
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum HIRDeclaration {
    HIRFunction(HIRFunction),
    HIRConst(HIRBinding),
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
    LiteralInt {
        value: u64,
    },
    LiteralBool(bool),
    LiteralString { value: String },
    Identifier(Identifier),
    Binary {
        left: Box<HIRExpression>,
        // TODO: turn this into Operation to decouple operations and operators
        operator: Operator,
        right: Box<HIRExpression>,
    },
    Call {
        callee: Identifier,
        args: Vec<HIRExpression>,
    },
    FieldAccess {
        object: Box<HIRExpression>,
        field: String,
        field_index: usize,
    },
    StructConstruct {
        type_name: String,
        fields: Vec<(String, HIRExpression)>,
    },
    Null,
}

#[derive(Debug, Clone)]
pub enum HIRStmt {
    Binding(HIRBinding),
    Assign {
        name: Identifier,
        expr: HIRExpression,
    },
    FieldAssign {
        object: Identifier,
        field: String,
        field_index: usize,
        expr: HIRExpression,
    },
    Expr(HIRExpression),
    Return(Option<HIRExpression>),
    If(HIRIf),
    For {
        init: Option<Box<HIRStmt>>,
        cond: Option<HIRExpression>,
        post: Option<Box<HIRStmt>>,
        body: Vec<HIRStmt>,
    },
    Break,
    Continue,
}

#[derive(Debug, Clone)]
pub struct HIRBinding {
    pub name: Identifier,
    pub ty: HIRTypeKind,
    pub init: Option<HIRExpression>,
}

#[derive(Debug, Clone)]
pub struct HIRIf {
    pub cond: HIRExpression,
    pub then_branch: Vec<HIRStmt>,
    pub else_branch: Option<Vec<HIRStmt>>,
}

impl HIRIf {
    pub fn then_branch_terminates(&self) -> bool {
        for stmt in self.then_branch.iter() {
            if matches!(stmt, HIRStmt::Return(_) | HIRStmt::Break | HIRStmt::Continue) {
                return true;
            }
        }
        return false;
    }

    pub fn else_branch_terminates(&self) -> bool {
        if let Some(eb) = &self.else_branch {
            for stmt in eb.iter() {
                if matches!(stmt, HIRStmt::Return(_) | HIRStmt::Break | HIRStmt::Continue) {
                    return true;
                }
            }
            return false;
        } else {
            return false;
        }
    }
}
