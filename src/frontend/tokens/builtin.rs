use core::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Builtin {
    BuiltinType(BuiltinType),
    BuiltinFunction(BuiltinFunction),
    /// A comptime property queried on a value, e.g. `arr.@len`. Only valid
    /// immediately after a `.`; a bare `@len` is a parse error.
    BuiltinProperty(BuiltinProperty),
}

impl Builtin {
    /// Map legacy `@`-prefixed builtin names after the old lexer consumes `@`.
    #[deprecated(since = "0.1.2")]
    pub fn from_name(name: &str) -> Option<Builtin> {
        if let Some(ty) = BuiltinType::from_legacy_name(name) {
            return Some(Builtin::BuiltinType(ty));
        }
        if let Some(func) = BuiltinFunction::from_name(name) {
            return Some(Builtin::BuiltinFunction(func));
        }
        if let Some(prop) = BuiltinProperty::from_name(name) {
            return Some(Builtin::BuiltinProperty(prop));
        }
        None
    }
}

#[derive(Debug, Clone, PartialEq)]
#[repr(u32)]
pub enum BuiltinType {
    Void = 0,
    UInt8 = 1,
    UInt16 = 2,
    UInt32 = 3,
    UInt64 = 4,
    UInt128 = 5,
    Int8 = 6,
    Int16 = 7,
    Int32 = 8,
    Int64 = 9,
    Int128 = 10,
    Float16 = 11,
    Float32 = 12,
    Float64 = 13,
    Float128 = 14,
    Char = 15,
    Boolean = 16,
    String = 17,
    Never = 18,
    /// Pointer-width unsigned integer. Maps to `uint64` (i64) on 64-bit
    /// targets; the length type returned by `@str_len`, `.len`, and `.@len`.
    Usize = 19,
}

impl BuiltinType {
    /// The canonical bit-width name of this type (without the legacy `@` prefix).
    pub fn name(&self) -> &'static str {
        match self {
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
            Self::Char => "char",
            Self::Boolean => "bool",
            Self::String => "string",
            Self::Never => "never",
            Self::Usize => "usize",
        }
    }

    /// Resolve a canonical bit-width type name or its default-width alias.
    pub fn from_name(name: &str) -> Option<BuiltinType> {
        Some(match name {
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
            "char" => Self::Char,
            "bool" => Self::Boolean,
            "string" => Self::String,
            "never" => Self::Never,
            "usize" => Self::Usize,
            _ => return None,
        })
    }

    /// Resolve byte-width spellings accepted by the production lexer until migration.
    pub(crate) fn from_legacy_name(name: &str) -> Option<BuiltinType> {
        Some(match name {
            "void" => Self::Void,
            "uint" | "uint4" => Self::UInt32,
            "uint2" => Self::UInt16,
            "uint8" => Self::UInt64,
            "uint16" => Self::UInt128,
            "int" | "int4" => Self::Int32,
            "int2" => Self::Int16,
            "int8" => Self::Int64,
            "int16" => Self::Int128,
            "float2" => Self::Float16,
            "float4" => Self::Float32,
            "float8" => Self::Float64,
            "float16" => Self::Float128,
            "char" => Self::Char,
            "bool" => Self::Boolean,
            "string" => Self::String,
            "never" => Self::Never,
            "usize" => Self::Usize,
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

/// Comptime properties queried on values with `.@name`. Unlike builtin types
/// and functions they must follow a `.`; the lexer still maps the bare name
/// (`@len`) so the parser can give targeted diagnostics.
#[derive(Debug, Clone, PartialEq)]
pub enum BuiltinProperty {
    /// `arr.@len` — the comptime element count of an array. Returns `@usize`.
    Len,
}

impl BuiltinProperty {
    /// The bare name of this property (without the leading `@`).
    pub fn name(&self) -> &'static str {
        match self {
            Self::Len => "len",
        }
    }

    /// Resolve a bare property name (without `@`) or `None`.
    pub fn from_name(name: &str) -> Option<BuiltinProperty> {
        Some(match name {
            "len" => Self::Len,
            _ => return None,
        })
    }
}

impl fmt::Display for BuiltinProperty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "@{}", self.name())
    }
}
