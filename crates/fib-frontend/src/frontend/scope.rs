use std::collections::{HashMap, hash_map::Entry};

use crate::frontend::{identifier::LabelId, scope_kind::ScopeKind, symbol_table::LocalBinding};

/// One lexical frame. File declarations and type members live elsewhere.
#[derive(Debug, Clone)]
pub struct Scope {
    kind: ScopeKind,
    bindings: HashMap<String, LocalBinding>,
    labels: HashMap<String, LabelId>,
}

impl Scope {
    pub(super) fn binding_names(&self) -> impl Iterator<Item = &String> {
        self.bindings.keys()
    }
    pub fn new(kind: ScopeKind) -> Self {
        Self {
            kind,
            bindings: HashMap::new(),
            labels: HashMap::new(),
        }
    }

    /// Inserts only in this frame. Cross-frame redeclaration policy belongs to
    /// the name pass; insertion never allocates or rewrites semantic identities.
    pub fn insert_binding(
        &mut self,
        spelling: String,
        binding: LocalBinding,
    ) -> anyhow::Result<()> {
        match self.bindings.entry(spelling) {
            Entry::Vacant(entry) => {
                entry.insert(binding);
                Ok(())
            }
            Entry::Occupied(entry) => anyhow::bail!("duplicate lexical binding '{}'", entry.key()),
        }
    }

    pub fn lookup_binding(&self, spelling: &str) -> Option<LocalBinding> {
        self.bindings.get(spelling).copied()
    }

    pub fn insert_label(&mut self, spelling: String, label: LabelId) -> anyhow::Result<()> {
        match self.labels.entry(spelling) {
            Entry::Vacant(entry) => {
                entry.insert(label);
                Ok(())
            }
            Entry::Occupied(entry) => {
                anyhow::bail!("duplicate control-flow label '{}'", entry.key())
            }
        }
    }

    pub fn lookup_label(&self, spelling: &str) -> Option<LabelId> {
        self.labels.get(spelling).copied()
    }

    pub fn get_kind(&self) -> ScopeKind {
        self.kind
    }
}

impl Default for Scope {
    fn default() -> Self {
        Self::new(ScopeKind::Module)
    }
}
