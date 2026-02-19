#[derive(Debug, Clone, PartialEq)]
pub enum TypeIdentifier {
    UserDefinedType(String),
    Unit,
    Integer,
    Float,
    String,
    Boolean,
    Character,
    Function {
        argument_types: Vec<TypeIdentifier>,
        return_type: Box<TypeIdentifier>,
    },
    Pointer {
        pointer_variant: PointerVariant,
        pointed_type: Box<TypeIdentifier>,
    },
    Struct {
        fields: Vec<Field>,
    },
    Variant {
        fields: Vec<Field>,
    },
    Dynamic,
    Blob,
    Never,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub(crate) label: String,
    pub(crate) type_id: TypeIdentifier,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PointerVariant {
    Unique,
    Shared,
    Weak,
    Raw,
}
