use core::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum BuiltinType {
    /// Represents that the value is itself a `type`.
    MetaType,
    Void,
    UInt8,
    UInt16,
    UInt32,
    UInt64,
    UInt128,
    Int8,
    Int16,
    Int32,
    Int64,
    Int128,
    Float16,
    Float32,
    Float64,
    Float128,
    Boolean,
    String,
    Never,
    /// Pointer-width unsigned integer. Maps to `uint64` (i64) on 64-bit
    /// targets; the symbolic length type returned by `.len`.
    Usize,
}

impl BuiltinType {
    /// The canonical source name of this type.
    pub fn name(&self) -> &'static str {
        match self {
            Self::MetaType => "type",
            Self::Void => "void",
            Self::UInt8 => "uint8",
            Self::UInt16 => "uint16",
            Self::UInt32 => "uint32",
            Self::UInt64 => "uint64",
            Self::UInt128 => "uint128",
            Self::Int8 => "int8",
            Self::Int16 => "int16",
            Self::Int32 => "int32",
            Self::Int64 => "int64",
            Self::Int128 => "int128",
            Self::Float16 => "float16",
            Self::Float32 => "float32",
            Self::Float64 => "float64",
            Self::Float128 => "float128",
            Self::Boolean => "bool",
            Self::String => "string",
            Self::Never => "never",
            Self::Usize => "usize",
        }
    }

    /// Resolve a canonical type name or its default-width alias.
    pub fn from_name(name: &str) -> Option<BuiltinType> {
        Some(match name {
            "type" => Self::MetaType,
            "void" => Self::Void,
            "uint" | "uint32" => Self::UInt32,
            "uint8" => Self::UInt8,
            "uint16" => Self::UInt16,
            "uint64" => Self::UInt64,
            "uint128" => Self::UInt128,
            "int" | "int32" => Self::Int32,
            "int8" => Self::Int8,
            "int16" => Self::Int16,
            "int64" => Self::Int64,
            "int128" => Self::Int128,
            "float16" => Self::Float16,
            "float32" => Self::Float32,
            "float64" => Self::Float64,
            "float128" => Self::Float128,
            "bool" => Self::Boolean,
            "never" => Self::Never,
            "usize" => Self::Usize,
            _ => return None,
        })
    }
}

impl fmt::Display for BuiltinType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}
