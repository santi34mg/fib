use core::fmt;
use std::collections::HashMap;

use crate::ast::ast::{FunctionDeclaration, ModulePath};
use crate::token::{Operator, builtin::BuiltinType, identifier::Identifier};

/// A resolved module — one `.fib` file's exported symbols.
#[derive(Debug, Clone)]
pub struct HIRModule {
    pub name: String,
    pub path: ModulePath,
    pub exports: HashMap<Identifier, HIRSymbol>,
    /// Declarations from this module that must be lowered into the final binary.
    pub declarations: Vec<HIRDeclaration>,
}

#[derive(Debug, Clone)]
pub struct CompilationUnit {
    pub scope_root: Scope,
    pub declarations: Vec<HIRDeclaration>,
    /// Declarations imported from other modules, also needing lowering.
    pub imported_declarations: Vec<HIRDeclaration>,
}

impl CompilationUnit {
    pub fn new() -> Self {
        return Self {
            scope_root: Scope::new(),
            declarations: Vec::new(),
            imported_declarations: Vec::new(),
        };
    }
}

#[derive(Debug, Clone)]
pub struct Scope {
    pub symbols: HashMap<Identifier, HIRSymbol>,
    /// Imported modules, keyed by local alias (last path segment or explicit alias).
    pub modules: HashMap<String, HIRModule>,
    pub children_scope: Vec<Box<Scope>>,
}

impl Scope {
    pub fn new() -> Self {
        return Self {
            symbols: HashMap::new(),
            modules: HashMap::new(),
            children_scope: Vec::new(),
        };
    }
}

#[derive(Debug, Clone)]
pub enum HIRSymbol {
    Type(HIRTypeKind),
    Function(HIRFunction),
    GenericFunction(GenericFunctionTemplate),
    Binding(HIRBinding),
}

/// A generic function template — a function with at least one `type`-typed parameter.
/// Not lowered directly; instantiated on demand when called with concrete type arguments.
#[derive(Debug, Clone)]
pub struct GenericFunctionTemplate {
    pub name: Identifier,
    pub ast_decl: FunctionDeclaration,
    /// Indices (into the original parameter list) of the compile-time `type` parameters.
    pub comptime_params: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HIRTypeKind {
    Builtin(BuiltinType),
    Identifier(Identifier),
    Struct { fields: Vec<(String, Box<HIRTypeKind>)> },
    Pointer(Box<HIRTypeKind>),
    Array {
        element_type: Box<HIRTypeKind>,
        size: u64,
    },
    Function {
        argument_types: Vec<HIRTypeKind>,
        return_type: Box<HIRTypeKind>,
    },
    /// A type from an imported module: `module::TypeName`
    QualifiedIdentifier {
        module: String,
        name: Identifier,
    },
    /// The metatype — the type of a compile-time type value. Never lowered to LLVM.
    Type,
}

impl fmt::Display for HIRTypeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Builtin(builtin) => write!(f, "{}", builtin)?,
            Self::Identifier(id) => write!(f, "{}", id)?,
            Self::Struct { fields } => write!(f, "struct {{ {:?} }}", fields)?,
            Self::Pointer(inner) => write!(f, "*{}", inner)?,
            Self::Array { element_type, size } => write!(f, "{}[{}]", element_type, size)?,
            Self::Function { argument_types, return_type } => write!(f, "fn({:?}) -> {}", argument_types, *return_type)?,
            Self::QualifiedIdentifier { module, name } => write!(f, "{}::{}", module, name)?,
            Self::Type => write!(f, "type")?,
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
    pub is_extern: bool,
    pub is_variadic: bool,
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
    LiteralFloat {
        value: f64,
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
    AddressOf(Box<HIRExpression>),
    Deref(Box<HIRExpression>),
    Cast {
        expr: Box<HIRExpression>,
        target_type: HIRTypeKind,
    },
    IndexAccess {
        object: Box<HIRExpression>,
        index: Box<HIRExpression>,
    },
    ArrayLiteral {
        elements: Vec<HIRExpression>,
    },
    /// A qualified reference to a symbol in an imported module: `module::member`
    QualifiedAccess {
        module: String,
        name: Identifier,
    },
    /// A compile-time type value. Consumed during analysis; never reaches LLVM lowering.
    ComptimeType(HIRTypeKind),
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
    Defer(Box<HIRStmt>),
    DerefAssign {
        pointer: HIRExpression,
        expr: HIRExpression,
    },
    IndexAssign {
        object: HIRExpression,
        index: HIRExpression,
        expr: HIRExpression,
    },
}

#[derive(Debug, Clone)]
pub struct HIRBinding {
    pub name: Identifier,
    pub ty: HIRTypeKind,
    pub init: Option<HIRExpression>,
    pub mutable: bool,
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
