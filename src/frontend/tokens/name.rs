use std::fmt::Display;

/// A symbol represents an identifier introduced by the user.
/// They are used for variable names, function names, type names
/// struct fields and enum variants.
#[derive(Debug, Clone, Eq, Hash, PartialEq)]
pub struct Name {
    pub name: String,
    pub id: u64,
}

impl Display for Name {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}
