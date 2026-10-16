use crate::frontend::tokens::name::Name;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeKind {
    Module(Name),
    Function,
    Block,
}
