//! Runtime operands and immutable definitions, not AST expression lowering.

use super::{BlockId, ConstantId, FunctionId, GlobalId, TypeId, ValueId};

/// No source symbols, implicit loads, or mutable temporaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operand {
    Value(ValueId),
    Constant(ConstantId),
}

/// An explicitly pointer-typed operand used as an address. This wrapper makes
/// memory operations visible; a future verifier must check its pointee type.
/// Constants may hold global addresses or null, never an implicit global load.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Address(pub Operand);

/// Exactly one definition and one runtime type for the lifetime of an SSA ID.
/// Type is stored once here rather than copied into each use of the ID.
#[derive(Debug, Clone)]
pub struct Value {
    pub owner: FunctionId,
    pub ty: TypeId,
    pub definition: ValueDefinition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueDefinition {
    BlockParameter {
        block: BlockId,
        index: usize,
    },
    InstructionResult {
        block: BlockId,
        instruction: usize,
        result: usize,
    },
}

#[derive(Debug, Clone)]
pub struct Constant {
    pub ty: TypeId,
    pub kind: ConstantKind,
}

/// Pure, module-wide data. No function-local values may occur in constants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstantKind {
    Bool(bool),
    /// Low bits of a typed integer; signed integers use two's complement.
    Integer {
        bits: u128,
    },
    /// IEEE bit pattern at the width specified by the constant's type.
    Float {
        bits: u128,
    },
    Null,
    /// Array, tuple or struct elements in type declaration order.
    Aggregate(Vec<ConstantId>),
    /// Variant index into the enum type, not its discriminant.
    Enum {
        variant: usize,
        fields: Vec<ConstantId>,
    },
    /// Exact bytes of an unsigned 8-bit array; no implicit NUL or allocation.
    Bytes(Vec<u8>),
    GlobalAddress(GlobalId),
    FunctionAddress(FunctionId),
}
