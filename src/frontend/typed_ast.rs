use core::fmt;
use std::collections::HashMap;

use crate::frontend::ast::{function_declaration::FunctionDeclaration, imports::ModulePath};
use crate::frontend::identifier::Identifier;
use crate::frontend::tokens::{
    Operator,
    builtin::{BuiltinFunction, BuiltinType},
};

/// A resolved module — one `.fib` file's exported symbols.
#[derive(Debug, Clone)]
pub struct TypedModule {
    pub name: String,
    pub path: ModulePath,
    pub exports: HashMap<Identifier, TypedSymbol>,
    /// Declarations from this module that must be lowered into the final binary.
    pub declarations: Vec<TypedDecl>,
}

#[derive(Debug, Clone)]
pub struct TypedProgram {
    pub symbol_table: SymbolTable,
    pub declarations: Vec<TypedDecl>,
    /// Declarations imported from other modules, also needing lowering.
    pub imported_declarations: Vec<TypedDecl>,
}

impl TypedProgram {
    pub fn new() -> Self {
        Self {
            symbol_table: SymbolTable::new(),
            declarations: Vec::new(),
            imported_declarations: Vec::new(),
        }
    }
}

impl Default for TypedProgram {
    fn default() -> Self {
        Self::new()
    }
}

/// Dragon-style symbol table: a stack of lexical scopes plus a module table.
///
/// Frame 0 is always the global/module scope. `enter_scope` pushes a new
/// (function or block) frame, `exit_scope` discards it so locals never leak.
/// `lookup` walks innermost -> outermost. Modules live once at the table
/// level (never cloned per block).
#[derive(Debug, Clone, Default)]
pub struct SymbolTable {
    scopes: Vec<HashMap<Identifier, TypedSymbol>>,
    modules: HashMap<String, TypedModule>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    Global,
    Function,
    Block,
}

impl SymbolTable {
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
            modules: HashMap::new(),
        }
    }

    pub fn enter_scope(&mut self, _kind: ScopeKind) {
        self.scopes.push(HashMap::new());
    }

    pub fn exit_scope(&mut self) {
        // Unbalanced scope exit is an internal compiler bug. Never panic in
        // library code: flag it in debug builds, no-op in release.
        debug_assert!(
            self.scopes.len() > 1,
            "SymbolTable::exit_scope called on global scope"
        );
        if self.scopes.len() <= 1 {
            return;
        }
        self.scopes.pop();
    }

    pub fn depth(&self) -> usize {
        self.scopes.len()
    }

    /// Insert into the innermost scope. Returns any previous binding in that frame.
    /// Returns `None` when there is no scope (defensive: the table always
    /// carries at least the global frame; never panics).
    pub fn insert(&mut self, id: Identifier, sym: TypedSymbol) -> Option<TypedSymbol> {
        self.scopes
            .last_mut()
            .and_then(|frame| frame.insert(id, sym))
    }

    /// Lexical lookup: innermost frame first.
    pub fn lookup(&self, id: &Identifier) -> Option<&TypedSymbol> {
        self.scopes.iter().rev().find_map(|frame| frame.get(id))
    }

    /// Remove from the innermost scope containing `id`. Returns the removed symbol.
    pub fn remove(&mut self, id: &Identifier) -> Option<TypedSymbol> {
        for frame in self.scopes.iter_mut().rev() {
            if let Some(sym) = frame.remove(id) {
                return Some(sym);
            }
        }
        None
    }

    /// Symbols declared at global scope (frame 0). Used for module exports.
    pub fn global_symbols(&self) -> &HashMap<Identifier, TypedSymbol> {
        &self.scopes[0]
    }

    pub fn insert_module(&mut self, alias: String, module: TypedModule) {
        self.modules.insert(alias, module);
    }

    pub fn lookup_module(&self, alias: &str) -> Option<&TypedModule> {
        self.modules.get(alias)
    }

    pub fn modules(&self) -> impl Iterator<Item = &TypedModule> {
        self.modules.values()
    }
}

#[derive(Debug, Clone)]
pub enum TypedSymbol {
    Type(Ty),
    Function(TypedFunction),
    GenericFunction(GenericFunctionTemplate),
    Binding(TypedBinding),
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
pub struct TypedEnumVariant {
    pub name: String,
    pub discriminant: u32,
    pub payload: Option<Vec<(String, Ty)>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Ty {
    Builtin(BuiltinType),
    Identifier(Identifier),
    Struct {
        fields: Vec<(String, Box<Ty>)>,
    },
    Enum {
        variants: Vec<TypedEnumVariant>,
    },
    Pointer(Box<Ty>),
    Array {
        element_type: Box<Ty>,
        size: u64,
    },
    Function {
        argument_types: Vec<Ty>,
        return_type: Box<Ty>,
    },
    Tuple {
        elements: Vec<Ty>,
    },
    /// A type from an imported module: `module::TypeName`
    QualifiedIdentifier {
        module: String,
        name: Identifier,
    },
    /// The metatype — the type of a compile-time type value. Never lowered to LLVM.
    Type,
}

impl fmt::Display for Ty {
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
pub struct TypedTypeDecl {
    pub name: Identifier,
    pub ty: Ty,
}

#[derive(Debug, Clone)]
pub enum TypedDecl {
    Function(TypedFunction),
    Const(TypedBinding),
    Type(TypedTypeDecl),
}

#[derive(Debug, Clone)]
pub struct TypedFunction {
    pub name: Identifier,
    pub params: Vec<(Identifier, Ty)>,
    pub return_type: Ty,
    pub body: Vec<TypedStatement>,
    pub is_extern: bool,
    pub is_variadic: bool,
}

#[derive(Debug, Clone)]
pub struct TypedExpr {
    pub inferred_type: Ty,
    pub expression: TypedExprKind,
}

#[derive(Debug, Clone)]
pub enum TypedExprKind {
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
        left: Box<TypedExpr>,
        // TODO: turn this into Operation to decouple operations and operators
        operator: Operator,
        right: Box<TypedExpr>,
    },
    Call {
        callee: Identifier,
        args: Vec<TypedExpr>,
    },
    /// A call to a builtin function, e.g. `@concat(a, b)`. Lowered directly to
    /// libc-backed LLVM IR rather than a user-defined function.
    BuiltinCall {
        builtin: BuiltinFunction,
        args: Vec<TypedExpr>,
    },
    FieldAccess {
        object: Box<TypedExpr>,
        field: String,
        field_index: usize,
    },
    StructConstruct {
        type_name: String,
        fields: Vec<(String, TypedExpr)>,
    },
    Null,
    AddressOf(Box<TypedExpr>),
    Deref(Box<TypedExpr>),
    Cast {
        expr: Box<TypedExpr>,
        target_type: Ty,
    },
    IndexAccess {
        object: Box<TypedExpr>,
        index: Box<TypedExpr>,
    },
    ArrayLiteral {
        elements: Vec<TypedExpr>,
    },
    /// A qualified reference to a symbol in an imported module: `module::member`
    QualifiedAccess {
        module: String,
        name: Identifier,
    },
    /// A compile-time type value. Consumed during analysis; never reaches LLVM lowering.
    ComptimeType(Ty),
    /// An enum variant value: `Color.Red`. The discriminant is the variant index.
    EnumLiteral {
        type_name: String,
        variant: String,
        discriminant: u32,
    },
    /// A tagged-union variant constructor: `Token.Integer { value: 42 }`.
    /// `enum_type` is the resolved `Ty::Enum` (so the lowering can
    /// compute the full enum struct without a scope lookup).
    EnumVariantConstruct {
        type_name: String,
        variant: String,
        discriminant: u32,
        fields: Vec<(String, TypedExpr)>,
        enum_type: Box<Ty>,
    },
}

#[derive(Debug, Clone)]
pub struct TypedReturn {
    pub values: Vec<TypedExpr>,
}

impl TypedReturn {
    /// First return value, if any. Prefer this over indexing: a bare
    /// `return;` carries an empty `values` vec and must not panic.
    pub fn first(&self) -> Option<&TypedExpr> {
        self.values.first()
    }
}

#[derive(Debug, Clone)]
pub enum TypedStatement {
    Binding(TypedBinding),
    Assign {
        name: Identifier,
        expr: TypedExpr,
    },
    MultiAssign {
        targets: Vec<TypedExpr>,
        values: Vec<TypedExpr>,
    },
    MultiBinding {
        bindings: Vec<TypedBinding>,
        values: Vec<TypedExpr>,
    },
    FieldAssign {
        object: TypedExpr,
        field: String,
        field_index: usize,
        expr: TypedExpr,
    },
    Expr(TypedExpr),
    Return(Option<TypedReturn>),
    If(TypedIf),
    For {
        init: Option<Box<TypedStatement>>,
        cond: Option<TypedExpr>,
        post: Option<Box<TypedStatement>>,
        body: Vec<TypedStatement>,
    },
    Break,
    Continue,
    Defer(Box<TypedStatement>),
    DerefAssign {
        pointer: TypedExpr,
        expr: TypedExpr,
    },
    IndexAssign {
        object: TypedExpr,
        index: TypedExpr,
        expr: TypedExpr,
    },
    Switch {
        subject: TypedExpr,
        arms: Vec<TypedSwitchArm>,
    },
}

#[derive(Debug, Clone)]
pub struct TypedSwitchArm {
    pub pattern: TypedPattern,
    pub body: Vec<TypedStatement>,
}

#[derive(Debug, Clone)]
pub enum TypedPattern {
    EnumVariant {
        variant: String,
        discriminant: u32,
        /// Local name to bind the payload to inside the arm body.
        binding: Option<Identifier>,
        /// Resolved payload struct type (for lowering / scope insertion). `None`
        /// when the variant carries no payload.
        payload_ty: Option<Ty>,
    },
    Wildcard,
}

#[derive(Debug, Clone)]
pub struct TypedBinding {
    pub name: Identifier,
    pub ty: Ty,
    pub init: Option<TypedExpr>,
    pub mutable: bool,
}

#[derive(Debug, Clone)]
pub struct TypedIf {
    pub cond: TypedExpr,
    pub then_branch: Vec<TypedStatement>,
    pub else_branch: Option<Vec<TypedStatement>>,
}

impl TypedIf {
    pub fn then_branch_terminates(&self) -> bool {
        for stmt in self.then_branch.iter() {
            if matches!(
                stmt,
                TypedStatement::Return(_) | TypedStatement::Break | TypedStatement::Continue
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
                    TypedStatement::Return(_) | TypedStatement::Break | TypedStatement::Continue
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
