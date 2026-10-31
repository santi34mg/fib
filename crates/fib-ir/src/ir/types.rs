//! IR-owned semantic types. Target layout and ABI legalization belong later;
//! these records contain neither frontend types nor LLVM handles.

use super::TypeId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrType {
    /// Unsized opaque pointee; never a value or memory access type.
    Opaque,
    Bool,
    /// Interpreter-owned immutable byte string. It has no native ABI layout yet.
    Bytes,
    Integer {
        width: IntegerWidth,
        signed: bool,
    },
    Float(FloatWidth),
    Pointer {
        pointee: TypeId,
    },
    FunctionPointer(Signature),
    Array {
        element: TypeId,
        len: u64,
    },
    Slice {
        element: TypeId,
    },
    Tuple(Vec<TypeId>),
    /// Identity is the TypeId, not the optional diagnostic name.
    Struct {
        name: Option<String>,
        fields: Vec<TypeId>,
    },
    Enum {
        name: Option<String>,
        variants: Vec<EnumVariant>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntegerWidth {
    /// Bit width, not bytes; the initial constant representation supports 1..=128.
    Bits(u16),
    Pointer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatWidth {
    F16,
    F32,
    F64,
    F128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumVariant {
    pub name: Option<String>,
    pub discriminant: u128,
    pub fields: Vec<TypeId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    pub params: Vec<TypeId>,
    /// Empty means void; multiple results remain explicit until ABI lowering.
    pub results: Vec<TypeId>,
    pub calling_convention: CallingConvention,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallingConvention {
    Fib,
    C,
}
