//! Non-terminating operations. Results define fresh SSA values, never places.

use super::{Address, ConstantId, FunctionId, GlobalId, Operand, StackSlotId, ValueId};

#[derive(Debug, Clone)]
pub struct Instruction {
    /// Most operations have one result; Store has none and Call follows its
    /// callee signature. Types/definition locations live in `Ir::values`.
    pub results: Vec<ValueId>,
    pub kind: InstructionKind,
}

#[derive(Debug, Clone)]
pub enum InstructionKind {
    Constant(ConstantId),
    /// Arithmetic/bitwise operands have the same type; comparisons return Bool.
    /// Signedness and float semantics come from the operand types, not the
    /// result type (which may be Bool).
    Binary {
        op: BinaryOp,
        lhs: Operand,
        rhs: Operand,
    },
    Unary {
        op: UnaryOp,
        operand: Operand,
    },
    /// Source type comes from the operand; destination from the result value.
    Cast {
        kind: CastKind,
        operand: Operand,
    },
    /// Canonical function identity; the declaration owns the full signature.
    Call {
        callee: FunctionId,
        args: Vec<Operand>,
    },
    StackAddress(StackSlotId),
    GlobalAddress(GlobalId),
    Load {
        address: Address,
    },
    Store {
        address: Address,
        value: Operand,
    },
    /// Aggregate operations manipulate immutable values, not memory.
    Aggregate {
        elements: Vec<Operand>,
    },
    ExtractField {
        aggregate: Operand,
        field: usize,
    },
    InsertField {
        aggregate: Operand,
        field: usize,
        value: Operand,
    },
    ExtractElement {
        aggregate: Operand,
        index: Operand,
    },
    /// Projection produces a pointer value; it does not read storage.
    FieldAddress {
        base: Address,
        field: usize,
    },
    /// Base points to an element; index is an element offset, not bytes.
    /// Bounds checks must be explicit control flow, never a hidden load/store.
    ElementAddress {
        base: Address,
        index: Operand,
    },
    Enum {
        variant: usize,
        fields: Vec<Operand>,
    },
    /// Result must be an integer capable of representing every discriminant.
    EnumTag {
        value: Operand,
    },
    /// The caller must first establish that the selected variant is active.
    EnumField {
        value: Operand,
        variant: usize,
        field: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
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
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CastKind {
    Numeric,
    Bitcast,
    PointerToInteger,
    IntegerToPointer,
}
