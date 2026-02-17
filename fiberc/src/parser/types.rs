#[derive(Debug, Clone, PartialEq)]
pub enum TypeIdentifier {
    UserDefinedType,
    Unit,
    Integer,
    Float,
    String,
    Boolean,
    Character,
    Pointer,
    Struct,
    Variant,
    Dynamic,
    Blob,
    Never,
}
