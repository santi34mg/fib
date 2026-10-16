use crate::frontend::{ast::qualified_name::QualifiedName, scope::Scope, scope_kind::ScopeKind};

/// Dragon-style symbol table: a stack of lexical scopes plus a module table.
///
/// Frame 0 is always the global/module scope. `enter_scope` pushes a new
/// (function or block) frame, `exit_scope` discards it so locals never leak.
/// `lookup` walks innermost -> outermost. Modules live once at the table
/// level (never cloned per block).
#[derive(Debug, Clone, Default)]
pub struct SymbolTable {
    scopes: Vec<Scope>,
    last_used_id: u64,
}

impl SymbolTable {
    pub fn new() -> Self {
        Self {
            scopes: vec![Scope::default()],
            last_used_id: 0,
        }
    }

    pub fn get_curr_scope_kind(&self) -> ScopeKind {
        // SAFETY(unwrap): should never have an empty table
        self.scopes.last().unwrap().get_kind()
    }

    pub fn enter_scope(&mut self, kind: ScopeKind) {
        self.scopes.push(Scope::new(kind));
    }

    pub fn exit_scope(&mut self) {
        // Unbalanced scope exit is an internal compiler bug. Never panic in
        // library code: flag it in debug builds, no-op in release.
        debug_assert!(
            self.scopes.len() > 1,
            "SymbolTable::exit_scope called on global scope"
        );
        if self.scopes.len() <= 1 {
            return;
        }
        self.scopes.pop();
    }

    pub fn depth(&self) -> usize {
        self.scopes.len()
    }

    /// Inserts a name into the scope, updating the ID of the symbol passed in
    /// in case a symbol with the same name was already present.
    pub fn insert_name(&mut self, qname: &mut QualifiedName) {
        self.scopes
            .last_mut()
            // SAFETY(unwrap): should never have an empty table
            .unwrap()
            .insert_name(qname)
    }
}
