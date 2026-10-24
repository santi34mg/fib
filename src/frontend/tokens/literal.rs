use std::hash::Hash;

#[derive(Debug, Clone)]
pub enum Literal {
    Integer(num_bigint::BigUint),
    Float(String),
    Boolean(bool),
    Character(char),
    String(String),
    Null,
}

impl PartialEq for Literal {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Integer(l0), Self::Integer(r0)) => l0 == r0,
            (Self::Float(l0), Self::Float(r0)) => l0 == r0,
            (Self::Boolean(l0), Self::Boolean(r0)) => l0 == r0,
            (Self::Character(l0), Self::Character(r0)) => l0 == r0,
            (Self::String(l0), Self::String(r0)) => l0 == r0,
            _ => core::mem::discriminant(self) == core::mem::discriminant(other),
        }
    }
}

impl Eq for Literal {}

impl Hash for Literal {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);

        match self {
            Self::Integer(value) => value.hash(state),
            Self::Float(value) => value.hash(state),
            Self::Boolean(value) => value.hash(state),
            Self::Character(value) => value.hash(state),
            Self::String(value) => value.hash(state),
            Self::Null => {}
        }
    }
}
