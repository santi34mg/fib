use crate::frontend::{
    identifier::{Entity, LocalId, ResolvedName},
    scope::Scope,
    scope_kind::ScopeKind,
};

pub use crate::frontend::identifier::LabelId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalBinding {
    Local(LocalId),
    CompAlias(Entity),
}

/// Lexical scopes only; the file-level registry lives in `ProjectSymbols`.
/// A permanent root frame makes `new` and `default` obey the same invariant.
/// Scope lookup is mechanical; declaration order, no-capture boundaries, and
/// redeclaration/label-target legality remain responsibilities of the passes.
#[derive(Debug, Clone)]
pub struct SymbolTable {
    root: Scope,
    scopes: Vec<Scope>,
}

impl SymbolTable {
    pub fn new() -> Self {
        Self {
            root: Scope::default(),
            scopes: Vec::new(),
        }
    }

    pub fn resolve_lexical(&self, spelling: &str) -> Option<ResolvedName> {
        self.scopes
            .iter()
            .rev()
            .chain(std::iter::once(&self.root))
            .find_map(|scope| scope.lookup_binding(spelling))
            .map(|binding| match binding {
                LocalBinding::Local(id) => ResolvedName::Local(id),
                LocalBinding::CompAlias(entity) => ResolvedName::Entity(entity),
            })
    }

    pub fn resolve_label(&self, spelling: &str) -> Option<LabelId> {
        self.scopes
            .iter()
            .rev()
            .chain(std::iter::once(&self.root))
            .find_map(|scope| scope.lookup_label(spelling))
    }

    pub fn get_curr_scope_kind(&self) -> ScopeKind {
        self.scopes.last().unwrap_or(&self.root).get_kind()
    }

    pub fn enter_scope(&mut self, kind: ScopeKind) {
        self.scopes.push(Scope::new(kind));
    }

    pub fn exit_scope(&mut self) -> anyhow::Result<()> {
        self.scopes
            .pop()
            .map(|_| ())
            .ok_or_else(|| anyhow::anyhow!("cannot exit the root lexical scope"))
    }

    pub fn depth(&self) -> usize {
        self.scopes.len() + 1
    }

    pub fn insert_binding(
        &mut self,
        spelling: String,
        binding: LocalBinding,
    ) -> anyhow::Result<()> {
        self.scopes
            .last_mut()
            .unwrap_or(&mut self.root)
            .insert_binding(spelling, binding)
    }

    pub fn insert_label(&mut self, spelling: String, label: LabelId) -> anyhow::Result<()> {
        self.scopes
            .last_mut()
            .unwrap_or(&mut self.root)
            .insert_label(spelling, label)
    }
}

impl Default for SymbolTable {
    fn default() -> Self {
        Self::new()
    }
}
