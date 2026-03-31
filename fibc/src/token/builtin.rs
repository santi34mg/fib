use core::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Builtin {
    BuiltinType(BuiltinType),
}

#[derive(Debug, Clone, PartialEq)]
#[repr(u32)]
pub enum BuiltinType {
    Void    = 0,
    UInt1   = 1,
    UInt2   = 2,
    UInt4   = 3,
    UInt8   = 4,
    UInt16  = 5,
    Int1    = 6,
    Int2    = 7,
    Int4    = 8,
    Int8    = 9,
    Int16   = 10,
    Float2  = 11,
    Float4  = 12,
    Float8  = 13,
    Float16 = 14,
    Char    = 15,
    Boolean = 16,
    String  = 17,
    Never   = 18,
}

impl fmt::Display for BuiltinType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Void => write!(f, "void"),
            Self::UInt1 => write!(f, "uint"),
            Self::UInt2 => write!(f, "uint2"),
            Self::UInt4 => write!(f, "uint4"),
            Self::UInt8 => write!(f, "uint8"),
            Self::UInt16 => write!(f, "uint16"),
            Self::Int1 => write!(f, "int"),
            Self::Int2 => write!(f, "int2"),
            Self::Int4 => write!(f, "int4"),
            Self::Int8 => write!(f, "int8"),
            Self::Int16 => write!(f, "int16"),
            Self::Float2 => write!(f, "float2"),
            Self::Float4 => write!(f, "float4"),
            Self::Float8 => write!(f, "float8"),
            Self::Float16 => write!(f, "float16"),
            Self::Char => write!(f, "char"),
            Self::Boolean => write!(f, "bool"),
            Self::String => write!(f, "string"),
            Self::Never => write!(f, "never"),
        }?;
        Ok(())
    }
}
