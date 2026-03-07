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
    SInt8   = 5,
    SInt16  = 6,
    SInt32  = 7,
    SInt64  = 8,
    Float16 = 9,
    Float32 = 10,
    Float64 = 11,
    Float128= 12,
    Char    = 13,
    Boolean = 14,
}

impl fmt::Display for BuiltinType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Void => write!(f, "void"),
            Self::UInt8 => write!(f, "uint8"),
            Self::UInt16 => write!(f, "uint16"),
            Self::UInt32 => write!(f, "uint32"),
            Self::UInt64 => write!(f, "uint64"),
            Self::SInt8 => write!(f, "sint8"),
            Self::SInt16 => write!(f, "sint16"),
            Self::SInt32 => write!(f, "sint32"),
            Self::SInt64 => write!(f, "sint64"),
            Self::Float16 => write!(f, "float16"),
            Self::Float32 => write!(f, "float32"),
            Self::Float64 => write!(f, "float64"),
            Self::Float128 => write!(f, "float128"),
            Self::Char => write!(f, "char"),
            Self::Boolean => write!(f, "bool"),
        }?;
        Ok(())
    }
}
