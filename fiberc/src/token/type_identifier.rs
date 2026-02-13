#[derive(Debug, Copy, Clone, PartialEq)]
pub enum TypeIdentifier {
    Integer,
    Boolean,
    Char,
    Unit,
    UserDefinedType,
}
