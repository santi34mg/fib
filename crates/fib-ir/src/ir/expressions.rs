//! Runtime operands and immutable definitions, not AST expression lowering.

use super::{BlockId, ConstantId, FunctionId, GlobalId, TypeId, ValueId};

/// No source symbols, implicit loads, or mutable temporaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operand {
    Value(ValueId),
    Constant(ConstantId),
}

/// An explicitly pointer-typed operand used as an address. This wrapper makes
/// memory operations visible; verification checks its pointee and operation types.
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

/// A module-wide constant value, independent of any function's runtime state.
/// Constants may reference other constants, globals, or functions, but never
/// function-local SSA values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstantKind {
    /// A boolean literal.
    Bool(bool),
    /// The low-order bits of an integer value; signed values use two's-complement
    /// representation. The constant's type determines the integer width.
    Integer {
        bits: u128,
    },
    /// The IEEE-754 bit pattern of a floating-point value. The constant's type
    /// determines the floating-point width.
    Float {
        bits: u128,
    },
    /// A null value of the constant's declared type.
    Null,
    /// An array, tuple, or struct value, with elements ordered as specified by
    /// the corresponding type declaration.
    Aggregate(Vec<ConstantId>),
    /// An enum value. `variant` is the variant's zero-based index in the enum
    /// definition (not its discriminant); `fields` contains that variant's
    /// payload values in declaration order.
    Enum {
        variant: usize,
        fields: Vec<ConstantId>,
    },
    /// The exact contents of an unsigned 8-bit array. No terminator or storage
    /// allocation is implied.
    Bytes(Vec<u8>),
    /// Immutable static NUL-terminated native storage; payload excludes NUL.
    CString(Vec<u8>),
    /// The address of a global, represented as a constant pointer value.
    GlobalAddress(GlobalId),
    PointerCast(ConstantId),
    /// Constant projections preserve relocations instead of serializing addresses.
    FieldAddress {
        base: ConstantId,
        field: usize,
    },
    ArrayData(ConstantId),
    ElementAddress {
        base: ConstantId,
        index: u64,
    },
    Slice {
        data: ConstantId,
        length: u64,
    },
    /// The address of a function, represented as a constant pointer value.
    FunctionAddress(FunctionId),
}
