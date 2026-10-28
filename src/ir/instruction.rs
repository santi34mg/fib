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
    /// result type (which may be Bool). Integer arithmetic wraps; division by
    /// zero and invalid shift counts trap. Signed MIN/-1 wraps, MIN%-1 is zero.
    /// Shift counts may have a different integer type and are checked before
    /// conversion to the left width. Float operations use IEEE nearest/ties-even.
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
    /// Numeric integer casts retain low bits or extend using source signedness.
    /// Float-to-integer truncates toward zero and traps on nonfinite or out-of-
    /// range results. Other numeric conversions round to nearest, ties to even.
    Cast {
        kind: CastKind,
        operand: Operand,
    },
    /// Canonical function identity; the declaration owns the full signature.
    Call {
        callee: FunctionId,
        args: Vec<Operand>,
    },
    /// Linux x86-64 syscall. Arguments are uint64 register words or typed
    /// pointers; result is raw int64. Always has observable memory effects.
    Syscall {
        number: Operand,
        args: Vec<Operand>,
    },
    /// Function values retain their full logical signature and convention.
    FunctionAddress(FunctionId),
    CallIndirect {
        callee: Operand,
        args: Vec<Operand>,
    },
    Slice {
        data: Operand,
        length: Operand,
    },
    SliceData(Operand),
    SliceLength(Operand),
    /// Convert the address of inline array storage to its first element.
    ArrayData(Address),
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
