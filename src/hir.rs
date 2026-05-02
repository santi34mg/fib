use core::fmt;
use std::collections::HashMap;

use crate::ast::{FunctionDeclaration, ModulePath};
use crate::tokens::{Operator, builtin::BuiltinType, identifier::Identifier};

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
        Self {
            scope_root: Scope::new(),
            declarations: Vec::new(),
            imported_declarations: Vec::new(),
        }
    }
}

impl Default for CompilationUnit {
    fn default() -> Self {
        Self::new()
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
        Self {
            symbols: HashMap::new(),
            modules: HashMap::new(),
            children_scope: Vec::new(),
        }
    }
}

impl Default for Scope {
    fn default() -> Self {
        Self::new()
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
pub struct HIREnumVariant {
    pub name: String,
    pub discriminant: u32,
    pub payload: Option<Vec<(String, HIRTypeKind)>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HIRTypeKind {
    Builtin(BuiltinType),
    Identifier(Identifier),
    Struct {
        fields: Vec<(String, Box<HIRTypeKind>)>,
    },
    Enum {
        variants: Vec<HIREnumVariant>,
    },
    Pointer(Box<HIRTypeKind>),
    Array {
        element_type: Box<HIRTypeKind>,
        size: u64,
    },
    Function {
        argument_types: Vec<HIRTypeKind>,
        return_type: Box<HIRTypeKind>,
    },
    Tuple {
        elements: Vec<HIRTypeKind>,
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
            Self::Enum { variants } => write!(f, "enum {{ {:?} }}", variants)?,
            Self::Pointer(inner) => write!(f, "*{}", inner)?,
            Self::Array { element_type, size } => write!(f, "{}[{}]", element_type, size)?,
            Self::Function {
                argument_types,
                return_type,
            } => write!(f, "fn({:?}) -> {}", argument_types, *return_type)?,
            Self::Tuple { elements } => write!(f, "({:?})", elements)?,
            Self::QualifiedIdentifier { module, name } => write!(f, "{}::{}", module, name)?,
            Self::Type => write!(f, "type")?,
        };
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct HIRTypeDeclaration {
    pub name: Identifier,
    pub ty: HIRTypeKind,
}

#[derive(Debug, Clone)]
pub enum HIRDeclaration {
    HIRFunction(HIRFunction),
    HIRConst(HIRBinding),
    HIRType(HIRTypeDeclaration),
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
    LiteralString {
        value: String,
    },
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
    /// An enum variant value: `Color.Red`. The discriminant is the variant index.
    EnumLiteral {
        type_name: String,
        variant: String,
        discriminant: u32,
    },
    /// A tagged-union variant constructor: `Token.Integer { value: 42 }`.
    /// `enum_type` is the resolved `HIRTypeKind::Enum` (so the lowering can
    /// compute the full enum struct without a scope lookup).
    EnumVariantConstruct {
        type_name: String,
        variant: String,
        discriminant: u32,
        fields: Vec<(String, HIRExpression)>,
        enum_type: Box<HIRTypeKind>,
    },
}

#[derive(Debug, Clone)]
pub struct HIRReturn {
    pub values: Vec<HIRExpression>,
}

impl std::ops::Deref for HIRReturn {
    type Target = HIRExpression;

    fn deref(&self) -> &Self::Target {
        &self.values[0]
    }
}

#[derive(Debug, Clone)]
pub enum HIRStmt {
    Binding(HIRBinding),
    Assign {
        name: Identifier,
        expr: HIRExpression,
    },
    MultiAssign {
        targets: Vec<HIRExpression>,
        values: Vec<HIRExpression>,
    },
    MultiBinding {
        bindings: Vec<HIRBinding>,
        values: Vec<HIRExpression>,
    },
    FieldAssign {
        object: HIRExpression,
        field: String,
        field_index: usize,
        expr: HIRExpression,
    },
    Expr(HIRExpression),
    Return(Option<HIRReturn>),
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
    Switch {
        subject: HIRExpression,
        arms: Vec<HIRSwitchArm>,
    },
}

#[derive(Debug, Clone)]
pub struct HIRSwitchArm {
    pub pattern: HIRPattern,
    pub body: Vec<HIRStmt>,
}

#[derive(Debug, Clone)]
pub enum HIRPattern {
    EnumVariant {
        variant: String,
        discriminant: u32,
        /// Local name to bind the payload to inside the arm body.
        binding: Option<Identifier>,
        /// Resolved payload struct type (for lowering / scope insertion). `None`
        /// when the variant carries no payload.
        payload_ty: Option<HIRTypeKind>,
    },
    Wildcard,
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
            if matches!(
                stmt,
                HIRStmt::Return(_) | HIRStmt::Break | HIRStmt::Continue
            ) {
                return true;
            }
        }
        false
    }

    pub fn else_branch_terminates(&self) -> bool {
        if let Some(eb) = &self.else_branch {
            for stmt in eb.iter() {
                if matches!(
                    stmt,
                    HIRStmt::Return(_) | HIRStmt::Break | HIRStmt::Continue
                ) {
                    return true;
                }
            }
            false
        } else {
            false
        }
    }
}
