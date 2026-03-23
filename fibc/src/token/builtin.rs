use core::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Builtin {
    BuiltinType(BuiltinType),
}

#[derive(Debug, Clone, PartialEq)]
#[repr(u32)]
pub enum BuiltinType {
    Void    = 0,
    UInt8   = 1,
    UInt16  = 2,
    UInt32  = 3,
    UInt64  = 4,
    Int8    = 5,
    Int16   = 6,
    Int32   = 7,
    Int64   = 8,
    Float16 = 9,
    Float32 = 10,
    Float64 = 11,
    Float128= 12,
    Char    = 13,
    Boolean = 14,
    String  = 15,
    Never   = 16,
}

impl fmt::Display for BuiltinType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Void => write!(f, "void"),
            Self::UInt8 => write!(f, "uint8"),
            Self::UInt16 => write!(f, "uint16"),
            Self::UInt32 => write!(f, "uint32"),
            Self::UInt64 => write!(f, "uint64"),
            Self::Int8 => write!(f, "int8"),
            Self::Int16 => write!(f, "int16"),
            Self::Int32 => write!(f, "int32"),
            Self::Int64 => write!(f, "int64"),
            Self::Float16 => write!(f, "float16"),
            Self::Float32 => write!(f, "float32"),
            Self::Float64 => write!(f, "float64"),
            Self::Float128 => write!(f, "float128"),
            Self::Char => write!(f, "char"),
            Self::Boolean => write!(f, "bool"),
            Self::String => write!(f, "string"),
            Self::Never => write!(f, "never"),
        }?;
        Ok(())
    }
}
