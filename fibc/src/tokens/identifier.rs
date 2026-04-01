use std::fmt::{Display, Formatter, Result};

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct Identifier {
    pub identifier: String,
}

impl Display for Identifier {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "{}", self.identifier)
    }
}

impl Identifier {
    pub fn new(identifier: String) -> Self {
        Self { identifier }
    }
}
