//! Dragon-style three-address IR (middle-end).
//!
//! Boundary: the frontend produces `typed_ast::TypedProgram` (trees +
//! `If`/`For`/`Switch`, nested expressions). This module is the first
//! flat representation: at most one operation per instruction, control
//! flow only via labels and jumps, values in temporaries.
//!
//! Status: `lower_typed_program` implements the core subset (locals,
//! integer/bool/float arithmetic, calls, `if`/`for`/`break`/`continue`,
//! `defer` via inline duplication at `return`/`break`/`continue`, `return`).
//! Aggregate / pointer / enum / `switch` / multi-value nodes are modeled
//! as explicit `Instruction` variants (design lock) but return
//! `LowerError::Unsupported` until phase 2b. `backend::lowering` still
//! consumes the typed AST directly; migration to consume `IrProgram`
//! happens after parity tests pass.

use std::fmt;

use crate::frontend::tokens::builtin::BuiltinFunction;
use crate::frontend::typed_ast::{Ty, TypedDecl, TypedProgram};
use statements::lower_function;

mod builder;
mod display;
mod expressions;
mod statements;
mod test;

/// A jump target. Labels are patched after emission (Dragon backpatching),
/// so jumps refer to a `Label` id, never to an `Instruction` by reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Label(pub u32);

/// A compiler-generated temporary holding one flat value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Temp(pub u32);

/// A resolved source binding (one `Alloca` slot). Unlike the old
/// `Place(String)`, this survives shadowing: each `Binding` allocates a
/// fresh `SymbolId`, and lexical scopes map names to the live id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SymbolId(pub u32);

/// An operand: a temporary, a resolved binding, or a constant.
#[derive(Debug, Clone)]
pub enum Operand {
    Temp(Temp),
    Symbol(SymbolId),
    ConstInt { value: u64, ty: Ty },
    ConstFloat { value: f64, ty: Ty },
    ConstBool(bool),
    StringLit(String),
    Null,
}

/// Generic binary operation. Signedness / float-ness is carried by the
/// `ty` field on `Instruction::Binary` so the backend can pick
/// `SDiv` vs `UDiv`, `SLT` vs `ULT`, `LShr` vs `AShr`, etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Shl,
    Shr,
    And,
    Or,
    Xor,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug, Clone)]
pub enum Instruction {
    /// Allocate a named slot. Emitted once per `Binding` (including params).
    Alloca {
        sym: SymbolId,
        name: String,
        ty: Ty,
    },
    Store {
        dst: SymbolId,
        src: Operand,
    },
    Load {
        dst: Temp,
        src: SymbolId,
    },
    Copy {
        dst: Temp,
        src: Operand,
    },
    Binary {
        dst: Temp,
        op: BinOp,
        lhs: Operand,
        rhs: Operand,
        /// Operand type (left's) for opcode selection: float vs int and
        /// signed vs unsigned (`SDiv` vs `UDiv`, `SLT` vs `ULT`, ...).
        /// For comparisons the result temp holds `@bool`.
        ty: Ty,
    },
    Unary {
        dst: Temp,
        op: UnOp,
        src: Operand,
        ty: Ty,
    },
    AddrOf {
        dst: Temp,
        src: SymbolId,
    },
    LoadPtr {
        dst: Temp,
        ptr: Operand,
        ty: Ty,
    },
    StorePtr {
        ptr: Operand,
        src: Operand,
    },
    Cast {
        dst: Temp,
        src: Operand,
        from: Ty,
        target: Ty,
    },
    Call {
        dst: Option<Temp>,
        callee: String,
        args: Vec<Operand>,
        ret_ty: Ty,
        is_variadic: bool,
    },
    BuiltinCall {
        dst: Option<Temp>,
        builtin: BuiltinFunction,
        args: Vec<Operand>,
        ret_ty: Ty,
    },
    // --- aggregates / enums (design-locked, lowering lands in phase 2b) ---
    StructConstruct {
        dst: Temp,
        type_name: String,
        fields: Vec<(String, Operand)>,
        ty: Ty,
    },
    FieldLoad {
        dst: Temp,
        base: Operand,
        field_index: usize,
        ty: Ty,
    },
    FieldStore {
        base: Operand,
        field_index: usize,
        src: Operand,
    },
    ArrayLiteral {
        dst: Temp,
        elems: Vec<Operand>,
        ty: Ty,
    },
    IndexLoad {
        dst: Temp,
        base: Operand,
        index: Operand,
        ty: Ty,
    },
    IndexStore {
        base: Operand,
        index: Operand,
        src: Operand,
    },
    EnumLiteral {
        dst: Temp,
        discriminant: u32,
        ty: Ty,
    },
    EnumConstruct {
        dst: Temp,
        discriminant: u32,
        payload: Vec<(String, Operand)>,
        ty: Ty,
    },
    SwitchDispatch {
        subject: Operand,
        /// (discriminant, target) pairs; backend emits `switch` on the tag.
        cases: Vec<(u32, Label)>,
        default: Label,
    },
    // --- control flow (flat only) ---
    IfGoto {
        cond: Operand,
        target: Label,
    },
    Goto {
        target: Label,
    },
    LabelDef(Label),
    Return {
        values: Vec<Operand>,
    },
    Unreachable,
}

#[derive(Debug, Clone)]
pub struct BasicBlock {
    pub label: Label,
    pub instrs: Vec<Instruction>,
}

#[derive(Debug, Clone)]
pub struct IrFunction {
    pub name: String,
    pub params: Vec<(SymbolId, String, Ty)>,
    pub return_ty: Ty,
    pub is_extern: bool,
    pub is_variadic: bool,
    pub temps: u32,
    pub symbols: Vec<(SymbolId, String, Ty)>,
    pub blocks: Vec<BasicBlock>,
}

#[derive(Debug, Clone, Default)]
pub struct IrProgram {
    pub functions: Vec<IrFunction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LowerError {
    Unsupported(String),
}

impl fmt::Display for LowerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(msg) => write!(f, "IR lowering unsupported: {}", msg),
        }
    }
}

impl std::error::Error for LowerError {}

// ---------------------------------------------------------------------------
// Predicates shared with the backend (single source of truth for signedness)
// ---------------------------------------------------------------------------

/// True for `@uint*` types. The backend uses this to pick `UDiv`/`URem`,
/// `ULT/UGT/...`, logical `LShr`, and `zext`/`uitofp` over signed forms.
pub fn ty_is_unsigned(ty: &Ty) -> bool {
    use crate::frontend::tokens::builtin::BuiltinType;
    matches!(
        ty,
        Ty::Builtin(
            BuiltinType::UInt1
                | BuiltinType::UInt2
                | BuiltinType::UInt4
                | BuiltinType::UInt8
                | BuiltinType::UInt16
        )
    )
}

pub fn ty_is_float(ty: &Ty) -> bool {
    use crate::frontend::tokens::builtin::BuiltinType;
    matches!(
        ty,
        Ty::Builtin(
            BuiltinType::Float2 | BuiltinType::Float4 | BuiltinType::Float8 | BuiltinType::Float16
        )
    )
}

/// Lower a typed program to flat IR.
///
/// Handles the core subset; everything else returns
/// `LowerError::Unsupported` with the source construct named.
pub fn lower_typed_program(program: TypedProgram) -> Result<IrProgram, LowerError> {
    let mut out = IrProgram::default();
    let mut decls = program.declarations;
    decls.extend(program.imported_declarations);
    for decl in &decls {
        match decl {
            TypedDecl::Function(f) => {
                out.functions.push(lower_function(f)?);
            }
            TypedDecl::Const(b) => {
                return Err(LowerError::Unsupported(format!(
                    "module-level const '{}' (lower via function bodies first)",
                    b.name
                )));
            }
            TypedDecl::Type(_) => {
                // Types live in the scope; no code to emit.
            }
        }
    }
    Ok(out)
}
