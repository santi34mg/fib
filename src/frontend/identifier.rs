//! Semantic identities, never source spellings or parser-assigned token IDs.

use crate::build_plan::FileId;

/// A declaration's stable slot in its owning file namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeclId {
    pub file_id: FileId,
    pub index: u32,
}

/// Unique within one analyzed project, including across functions and files.
/// Allocation belongs to analysis, not tokenization or scope insertion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LocalId(pub u32);

/// Unique within one analyzed project; never denotes a value or declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LabelId(pub u32);

/// A compile-time alias can denote a file or declaration, but never a local.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Entity {
    File(FileId),
    Declaration(DeclId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResolvedName {
    Entity(Entity),
    Local(LocalId),
}
