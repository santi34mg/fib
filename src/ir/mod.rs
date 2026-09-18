//! Dragon-style three-address IR (middle-end).
//!
//! Boundary: the frontend produces `typed_ast::TypedProgram` (trees +
//! `If`/`For`/`Switch`, nested expressions). This module is the first
//! flat representation: at most one operation per instruction, control
//! flow only via labels and jumps, values in temporaries.
//!
//! Status: skeleton only. `lower_typed_program` is not implemented yet;
//! `backend::lowering` still consumes the typed AST directly.

use crate::frontend::typed_ast::Ty;

/// A jump target. Labels are patched after emission (Dragon backpatching),
/// so jumps refer to a `Label` id, never to an `Instruction` by reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Label(pub u32);

/// A compiler-generated temporary holding one flat value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Temp(pub u32);

/// An operand: a source-level place, a temporary, or a constant.
#[derive(Debug, Clone)]
pub enum Operand {
    Temp(Temp),
    /// Named source binding (by name until we have SymbolIds).
    Place(String),
    ConstInt { value: u64, ty: Ty },
    ConstFloat { value: f64, ty: Ty },
    ConstBool(bool),
    StringLit(String),
    Null,
}

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
    Copy {
        dst: Temp,
        src: Operand,
    },
    Binary {
        dst: Temp,
        op: BinOp,
        lhs: Operand,
        rhs: Operand,
    },
    Unary {
        dst: Temp,
        op: UnOp,
        src: Operand,
    },
    IfGoto {
        cond: Operand,
        target: Label,
    },
    Goto {
        target: Label,
    },
    LabelDef(Label),
    Call {
        dst: Option<Temp>,
        callee: String,
        args: Vec<Operand>,
    },
    Return {
        values: Vec<Operand>,
    },
}

#[derive(Debug, Clone)]
pub struct BasicBlock {
    pub label: Label,
    pub instrs: Vec<Instruction>,
}

#[derive(Debug, Clone)]
pub struct IrFunction {
    pub name: String,
    pub params: Vec<(String, Ty)>,
    pub return_ty: Ty,
    pub temps: u32,
    pub blocks: Vec<BasicBlock>,
}

#[derive(Debug, Clone, Default)]
pub struct IrProgram {
    pub functions: Vec<IrFunction>,
}
