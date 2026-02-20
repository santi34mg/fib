use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Int,
    Bool,
    Unit,
    Function { args: Vec<Type>, ret: Box<Type> },
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Int => write!(f, "int"),
            Type::Bool => write!(f, "bool"),
            Type::Unit => write!(f, "unit"),
            Type::Function { args, ret } => {
                let args_s: Vec<String> = args.iter().map(|a| format!("{}", a)).collect();
                write!(f, "function({}) -> {}", args_s.join(", "), ret)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct HIRFunction {
    pub name: String,
    pub params: Vec<(String, Type)>,
    pub ret_type: Type,
    pub body: Vec<HIRStmt>,
}

#[derive(Debug, Clone)]
pub enum HIRExpr {
    LiteralInt(u32),
    LiteralBool(bool),
    Var(String),
    Binary {
        left: Box<HIRExpr>,
        op: String,
        right: Box<HIRExpr>,
    },
    Call {
        callee: String,
        args: Vec<HIRExpr>,
    },
    Null,
}

#[derive(Debug, Clone)]
pub enum HIRStmt {
    Let {
        name: String,
        ty: Option<Type>,
        init: Option<HIRExpr>,
    },
    Assign {
        name: String,
        expr: HIRExpr,
    },
    Expr(HIRExpr),
    Return(Option<HIRExpr>),
    If {
        cond: HIRExpr,
        then_branch: Vec<HIRStmt>,
        else_branch: Option<Vec<HIRStmt>>,
    },
    For {
        init: Option<Box<HIRStmt>>,
        cond: Option<HIRExpr>,
        post: Option<Box<HIRStmt>>,
        body: Vec<HIRStmt>,
    },
}
