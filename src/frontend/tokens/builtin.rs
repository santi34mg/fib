use core::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Builtin {
    BuiltinType(BuiltinType),
    BuiltinFunction(BuiltinFunction),
}

impl Builtin {
    /// Map a bare builtin name (without the leading `@`) to a builtin token.
    /// This is the single source of truth for which identifiers are builtins;
    /// the lexer consults it after consuming an `@`.
    pub fn from_name(name: &str) -> Option<Builtin> {
        if let Some(ty) = BuiltinType::from_name(name) {
            return Some(Builtin::BuiltinType(ty));
        }
        if let Some(func) = BuiltinFunction::from_name(name) {
            return Some(Builtin::BuiltinFunction(func));
        }
        None
    }
}

#[derive(Debug, Clone, PartialEq)]
#[repr(u32)]
pub enum BuiltinType {
    Void = 0,
    UInt1 = 1,
    UInt2 = 2,
    UInt4 = 3,
    UInt8 = 4,
    UInt16 = 5,
    Int1 = 6,
    Int2 = 7,
    Int4 = 8,
    Int8 = 9,
    Int16 = 10,
    Float2 = 11,
    Float4 = 12,
    Float8 = 13,
    Float16 = 14,
    Char = 15,
    Boolean = 16,
    String = 17,
    Never = 18,
}

impl BuiltinType {
    /// The bare name of this type (without the leading `@`).
    pub fn name(&self) -> &'static str {
        match self {
            Self::Void => "void",
            Self::UInt1 => "uint",
            Self::UInt2 => "uint2",
            Self::UInt4 => "uint4",
            Self::UInt8 => "uint8",
            Self::UInt16 => "uint16",
            Self::Int1 => "int",
            Self::Int2 => "int2",
            Self::Int4 => "int4",
            Self::Int8 => "int8",
            Self::Int16 => "int16",
            Self::Float2 => "float2",
            Self::Float4 => "float4",
            Self::Float8 => "float8",
            Self::Float16 => "float16",
            Self::Char => "char",
            Self::Boolean => "bool",
            Self::String => "string",
            Self::Never => "never",
        }
    }

    /// Resolve a bare type name (without `@`) to a `BuiltinType`.
    pub fn from_name(name: &str) -> Option<BuiltinType> {
        Some(match name {
            "void" => Self::Void,
            "uint" => Self::UInt1,
            "uint2" => Self::UInt2,
            "uint4" => Self::UInt4,
            "uint8" => Self::UInt8,
            "uint16" => Self::UInt16,
            "int" => Self::Int1,
            "int2" => Self::Int2,
            "int4" => Self::Int4,
            "int8" => Self::Int8,
            "int16" => Self::Int16,
            "float2" => Self::Float2,
            "float4" => Self::Float4,
            "float8" => Self::Float8,
            "float16" => Self::Float16,
            "char" => Self::Char,
            "bool" => Self::Boolean,
            "string" => Self::String,
            "never" => Self::Never,
            _ => return None,
        })
    }
}

impl fmt::Display for BuiltinType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Builtins are spelled with a leading `@` in fib source.
        write!(f, "@{}", self.name())
    }
}

/// Builtin functions, primarily for string building. Spelled `@name` in source.
#[derive(Debug, Clone, PartialEq)]
pub enum BuiltinFunction {
    /// `@concat(a: string, b: string) string` — heap-allocate the concatenation.
    Concat,
    /// `@str_len(s: string) uint8` — byte length of a string.
    StrLen,
    /// `@str_eq(a: string, b: string) bool` — byte-wise string equality.
    StrEq,
}

impl BuiltinFunction {
    /// The bare name of this function (without the leading `@`).
    pub fn name(&self) -> &'static str {
        match self {
            Self::Concat => "concat",
            Self::StrLen => "str_len",
            Self::StrEq => "str_eq",
        }
    }

    /// Resolve a bare function name (without `@`) to a `BuiltinFunction`.
    pub fn from_name(name: &str) -> Option<BuiltinFunction> {
        Some(match name {
            "concat" => Self::Concat,
            "str_len" => Self::StrLen,
            "str_eq" => Self::StrEq,
            _ => return None,
        })
    }
}

impl fmt::Display for BuiltinFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "@{}", self.name())
    }
}
